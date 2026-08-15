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

type Params = { params: Promise<{ tokenId: string }> };

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

async function proxyAgentToken(
  method: "DELETE",
  tokenId: string,
  accessToken: string,
): Promise<Response> {
  return fetch(syncServerUrl(`/api/admin/agent-tokens/${tokenId}`), {
    method,
    headers: {
      Authorization: `Bearer ${accessToken}`,
      Accept: "application/json",
    },
    cache: "no-store",
  });
}

async function withRefreshRetry(
  tokenId: string,
  context: RequestContext,
): Promise<{ response: Response; refreshed: RefreshResponse | null }> {
  if (!context.access) {
    return { response: new Response(null, { status: 401 }), refreshed: null };
  }

  let response = await proxyAgentToken("DELETE", tokenId, context.access);
  if (response.status !== 401 || !context.refresh) {
    return { response, refreshed: null };
  }

  const refreshed = await refreshTokens(context.refresh);
  if (!refreshed) {
    return { response, refreshed: null };
  }

  response = await proxyAgentToken("DELETE", tokenId, refreshed.access_token);
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

export async function DELETE(request: Request, { params }: Params) {
  if (!assertSameOrigin(request)) {
    return NextResponse.json({ error: "Forbidden" }, { status: 403 });
  }

  const { tokenId } = await params;
  const context = await getRequestContext();
  const { response, refreshed } = await withRefreshRetry(tokenId, context);

  if (!response.ok) {
    const body = await response.text();
    return NextResponse.json(
      { error: body || "Failed to revoke agent token" },
      { status: response.status === 401 ? 401 : response.status === 404 ? 404 : 400 },
    );
  }

  const next = NextResponse.json(await response.json());
  return withUpdatedCookies(next, refreshed);
}
