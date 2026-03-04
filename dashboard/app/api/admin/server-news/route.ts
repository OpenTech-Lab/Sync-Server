import { cookies } from "next/headers";
import { NextResponse } from "next/server";

import { ACCESS_COOKIE, REFRESH_COOKIE, syncServerUrl } from "@/lib/server-api";
import { assertSameOrigin } from "@/lib/security";

type RefreshResponse = {
  access_token: string;
  refresh_token: string;
  expires_in: number;
};

const secure = process.env.NODE_ENV === "production";

type RequestContext = {
  access: string | null;
  refresh: string | null;
};

async function getRequestContext(): Promise<RequestContext> {
  const jar = await cookies();
  return {
    access: jar.get(ACCESS_COOKIE)?.value ?? null,
    refresh: jar.get(REFRESH_COOKIE)?.value ?? null,
  };
}

async function refreshTokens(refresh: string): Promise<RefreshResponse | null> {
  const refreshResponse = await fetch(syncServerUrl("/auth/refresh"), {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Accept: "application/json",
    },
    body: JSON.stringify({ refresh_token: refresh }),
    cache: "no-store",
  });

  if (!refreshResponse.ok) {
    return null;
  }

  return (await refreshResponse.json()) as RefreshResponse;
}

async function proxyServerNews(
  method: "GET" | "POST",
  accessToken: string,
  payload?: unknown,
): Promise<Response> {
  return fetch(syncServerUrl("/api/admin/server-news"), {
    method,
    headers: {
      Authorization: `Bearer ${accessToken}`,
      "Content-Type": "application/json",
      Accept: "application/json",
    },
    body: payload == null ? undefined : JSON.stringify(payload),
    cache: "no-store",
  });
}

async function withRefreshRetry(
  method: "GET" | "POST",
  context: RequestContext,
  payload?: unknown,
): Promise<{ response: Response; refreshed: RefreshResponse | null }> {
  if (!context.access) {
    return { response: new Response(null, { status: 401 }), refreshed: null };
  }

  let response = await proxyServerNews(method, context.access, payload);
  if (response.status !== 401 || !context.refresh) {
    return { response, refreshed: null };
  }

  const refreshed = await refreshTokens(context.refresh);
  if (!refreshed) {
    return { response, refreshed: null };
  }

  response = await proxyServerNews(method, refreshed.access_token, payload);
  return { response, refreshed };
}

function withUpdatedCookies(next: NextResponse, refreshed: RefreshResponse | null): NextResponse {
  if (!refreshed) {
    return next;
  }

  next.cookies.set(ACCESS_COOKIE, refreshed.access_token, {
    httpOnly: true,
    sameSite: "strict",
    secure,
    path: "/",
    maxAge: refreshed.expires_in,
  });
  next.cookies.set(REFRESH_COOKIE, refreshed.refresh_token, {
    httpOnly: true,
    sameSite: "strict",
    secure,
    path: "/",
    maxAge: 60 * 60 * 24 * 30,
  });
  return next;
}

export async function GET() {
  const context = await getRequestContext();
  const { response, refreshed } = await withRefreshRetry("GET", context);

  if (!response.ok) {
    const body = await response.text();
    return NextResponse.json(
      { error: body || "Failed to load server news" },
      { status: response.status === 401 ? 401 : 400 },
    );
  }

  const next = NextResponse.json(await response.json());
  return withUpdatedCookies(next, refreshed);
}

export async function POST(request: Request) {
  if (!assertSameOrigin(request)) {
    return NextResponse.json({ error: "Forbidden" }, { status: 403 });
  }

  const payload = await request.json();
  const context = await getRequestContext();
  const { response, refreshed } = await withRefreshRetry("POST", context, payload);

  if (!response.ok) {
    const body = await response.text();
    return NextResponse.json(
      { error: body || "Failed to create server news" },
      { status: response.status === 401 ? 401 : 400 },
    );
  }

  const next = NextResponse.json(await response.json(), { status: 201 });
  return withUpdatedCookies(next, refreshed);
}
