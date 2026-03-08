import { NextRequest, NextResponse } from "next/server";

const ACCESS_COOKIE = "sync_dashboard_access_token";
const REFRESH_COOKIE = "sync_dashboard_refresh_token";

const REFRESH_BUFFER_SECS = 60;

const serverBaseUrl =
  process.env.SYNC_SERVER_URL?.trim() || "http://localhost:8080";

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

function isExpired(exp: number): boolean {
  return Date.now() / 1000 + REFRESH_BUFFER_SECS >= exp;
}

function buildCookieHeader(
  original: string,
  name: string,
  value: string,
): string {
  // Replace the named cookie's value in the cookie header string.
  // If the cookie is not present, append it.
  const re = new RegExp(`(?:^|;\\s*)${name}=[^;]*`);
  const replacement = `${name}=${value}`;
  if (re.test(original)) {
    return original.replace(re, (match) =>
      match.startsWith(";") ? `; ${replacement}` : replacement,
    );
  }
  return original ? `${original}; ${replacement}` : replacement;
}

export async function middleware(req: NextRequest) {
  const accessToken = req.cookies.get(ACCESS_COOKIE)?.value;
  const refreshToken = req.cookies.get(REFRESH_COOKIE)?.value;

  // If no refresh token, we cannot help — let the page handle it.
  if (!refreshToken) {
    return NextResponse.next();
  }

  // If access token is still valid, pass through.
  if (accessToken) {
    const exp = jwtExpiry(accessToken);
    if (exp !== null && !isExpired(exp)) {
      return NextResponse.next();
    }
  }

  // Access token is missing or expired — attempt refresh.
  let newAccess: string;
  let newRefresh: string;
  let expiresIn: number;

  try {
    const resp = await fetch(`${serverBaseUrl}/auth/refresh`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ refresh_token: refreshToken }),
    });

    if (!resp.ok) {
      // Refresh rejected — redirect to login.
      return NextResponse.redirect(new URL("/login", req.url));
    }

    const data = (await resp.json()) as {
      access_token: string;
      refresh_token: string;
      expires_in: number;
    };
    newAccess = data.access_token;
    newRefresh = data.refresh_token;
    expiresIn = data.expires_in;
  } catch {
    // Network error — let the request through; the page will redirect to login
    // if needed, avoiding a broken middleware redirect.
    return NextResponse.next();
  }

  // Mutate the Cookie header so Server Components (which read from the incoming
  // request) see the fresh access token without waiting for a round-trip.
  const requestHeaders = new Headers(req.headers);
  const currentCookies = requestHeaders.get("cookie") ?? "";
  requestHeaders.set(
    "cookie",
    buildCookieHeader(
      buildCookieHeader(currentCookies, ACCESS_COOKIE, newAccess),
      REFRESH_COOKIE,
      newRefresh,
    ),
  );

  const response = NextResponse.next({ request: { headers: requestHeaders } });

  const secure = req.url.startsWith("https://");

  response.cookies.set(ACCESS_COOKIE, newAccess, {
    httpOnly: true,
    sameSite: "strict",
    secure,
    path: "/",
    maxAge: expiresIn,
  });

  response.cookies.set(REFRESH_COOKIE, newRefresh, {
    httpOnly: true,
    sameSite: "strict",
    secure,
    path: "/",
    maxAge: 60 * 60 * 24 * 30,
  });

  return response;
}

export const config = {
  matcher: [
    // Run on all routes except static assets, Next internals, and public auth pages.
    "/((?!login|forgot-password|setup-admin|api|_next/static|_next/image|favicon\\.ico).*)",
  ],
};
