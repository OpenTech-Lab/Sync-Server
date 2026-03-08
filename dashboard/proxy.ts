import type { NextRequest } from "next/server";
import { NextResponse } from "next/server";

import { fetchSetupStatus } from "./lib/setup-status";
import { ACCESS_COOKIE, REFRESH_COOKIE } from "@/lib/server-api";

const REFRESH_BUFFER_SECS = 60;

const serverBaseUrl =
  process.env.SYNC_SERVER_URL?.trim() || "http://localhost:8080";

function secureHeaders(response: NextResponse): NextResponse {
  response.headers.set("X-Frame-Options", "DENY");
  response.headers.set("X-Content-Type-Options", "nosniff");
  response.headers.set("Referrer-Policy", "strict-origin-when-cross-origin");
  response.headers.set("Permissions-Policy", "camera=(), microphone=(), geolocation=()");
  response.headers.set(
    "Content-Security-Policy",
    "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' http: https:; frame-ancestors 'none'; base-uri 'self'; form-action 'self'",
  );
  return response;
}

function jwtExpiry(token: string): number | null {
  try {
    const parts = token.split(".");
    if (parts.length !== 3) return null;
    const payload = JSON.parse(
      Buffer.from(parts[1], "base64url").toString("utf8"),
    ) as { exp?: number };
    return typeof payload.exp === "number" ? payload.exp : null;
  } catch {
    return null;
  }
}

function isTokenExpired(exp: number): boolean {
  return Date.now() / 1000 + REFRESH_BUFFER_SECS >= exp;
}

function replaceCookie(cookieHeader: string, name: string, value: string): string {
  const re = new RegExp(`(?:^|;\\s*)${name}=[^;]*`);
  const replacement = `${name}=${value}`;
  if (re.test(cookieHeader)) {
    return cookieHeader.replace(re, (match) =>
      match.startsWith(";") ? `; ${replacement}` : replacement,
    );
  }
  return cookieHeader ? `${cookieHeader}; ${replacement}` : replacement;
}

async function tryRefreshTokens(
  request: NextRequest,
): Promise<{ access: string; refresh: string; expiresIn: number } | null> {
  const refreshToken = request.cookies.get(REFRESH_COOKIE)?.value;
  if (!refreshToken) return null;

  try {
    const resp = await fetch(`${serverBaseUrl}/auth/refresh`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ refresh_token: refreshToken }),
    });
    if (!resp.ok) return null;

    const data = (await resp.json()) as {
      access_token: string;
      refresh_token: string;
      expires_in: number;
    };
    return { access: data.access_token, refresh: data.refresh_token, expiresIn: data.expires_in };
  } catch {
    return null;
  }
}

export async function proxy(request: NextRequest) {
  const path = request.nextUrl.pathname;
  const basePath = "";
  const normalizedPath = path;
  const { needsSetup } = await fetchSetupStatus();

  const isSetupPath =
    normalizedPath === "/setup-admin" ||
    normalizedPath.startsWith("/setup-admin/");
  const isSetupApiPath = normalizedPath === "/api/session/setup-admin";

  if (needsSetup && !isSetupPath && !isSetupApiPath) {
    return secureHeaders(
      NextResponse.redirect(
        new URL(`${basePath}/setup-admin`, request.url),
        { status: 307 },
      ),
    );
  }

  if (!needsSetup && isSetupPath) {
    return secureHeaders(
      NextResponse.redirect(
        new URL(`${basePath}/login`, request.url),
        {
          status: 307,
        },
      ),
    );
  }

  const isProtectedPath =
    normalizedPath.startsWith("/dashboard") ||
    normalizedPath.startsWith("/users") ||
    normalizedPath.startsWith("/trust") ||
    normalizedPath.startsWith("/config") ||
    normalizedPath.startsWith("/audit") ||
    normalizedPath.startsWith("/stickers") ||
    normalizedPath.startsWith("/planet-news");

  const accessToken = request.cookies.get(ACCESS_COOKIE)?.value;

  if (isProtectedPath) {
    // Determine if we need a token refresh.
    const needsRefresh = !accessToken || (() => {
      const exp = jwtExpiry(accessToken);
      return exp === null || isTokenExpired(exp);
    })();

    if (needsRefresh) {
      const tokens = await tryRefreshTokens(request);

      if (!tokens) {
        // No refresh token or refresh failed — send to login.
        return secureHeaders(
          NextResponse.redirect(new URL(`${basePath}/login`, request.url), { status: 307 }),
        );
      }

      // Inject new access token into the request Cookie header so Server
      // Components that call cookies() see the fresh token immediately.
      const requestHeaders = new Headers(request.headers);
      const currentCookies = requestHeaders.get("cookie") ?? "";
      requestHeaders.set(
        "cookie",
        replaceCookie(
          replaceCookie(currentCookies, ACCESS_COOKIE, tokens.access),
          REFRESH_COOKIE,
          tokens.refresh,
        ),
      );

      const response = secureHeaders(
        NextResponse.next({ request: { headers: requestHeaders } }),
      );

      const secure = request.url.startsWith("https://");
      response.cookies.set(ACCESS_COOKIE, tokens.access, {
        httpOnly: true,
        sameSite: "strict",
        secure,
        path: "/",
        maxAge: tokens.expiresIn,
      });
      response.cookies.set(REFRESH_COOKIE, tokens.refresh, {
        httpOnly: true,
        sameSite: "strict",
        secure,
        path: "/",
        maxAge: 60 * 60 * 24 * 30,
      });

      return response;
    }
  }

  // Legacy guard: redirect to login if no session cookie at all on protected paths.
  if (isProtectedPath && !accessToken) {
    return secureHeaders(
      NextResponse.redirect(new URL(`${basePath}/login`, request.url), {
        status: 307,
      }),
    );
  }

  return secureHeaders(NextResponse.next());
}

export const config = {
  matcher: ["/((?!_next/static|_next/image|favicon.ico).*)"],
};
