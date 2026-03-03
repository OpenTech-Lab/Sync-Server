import type { NextRequest } from "next/server";
import { NextResponse } from "next/server";

import { fetchSetupStatus } from "./lib/setup-status";
import { ACCESS_COOKIE } from "@/lib/server-api";

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

export async function proxy(request: NextRequest) {
  const path = request.nextUrl.pathname;
  const basePath = "";
  const normalizedPath = path;
  const hasSession = Boolean(request.cookies.get(ACCESS_COOKIE)?.value);
  const { needsSetup } = await fetchSetupStatus();

  const isSetupPath = normalizedPath === "/setup-admin";
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
        new URL(`${basePath}${hasSession ? "/dashboard" : "/login"}`, request.url),
        {
          status: 307,
        },
      ),
    );
  }

  const isProtectedPath =
    normalizedPath.startsWith("/dashboard") ||
    normalizedPath.startsWith("/users") ||
    normalizedPath.startsWith("/config") ||
    normalizedPath.startsWith("/audit") ||
    normalizedPath.startsWith("/stickers");

  if (isProtectedPath && !hasSession) {
    return secureHeaders(
      NextResponse.redirect(new URL(`${basePath}/login`, request.url), {
        status: 307,
      }),
    );
  }

  if (normalizedPath === "/login" && hasSession) {
    return secureHeaders(
      NextResponse.redirect(new URL(`${basePath}/dashboard`, request.url), {
        status: 307,
      }),
    );
  }

  return secureHeaders(NextResponse.next());
}

export const config = {
  matcher: ["/((?!_next/static|_next/image|favicon.ico).*)"],
};
