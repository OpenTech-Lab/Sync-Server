import { NextResponse } from "next/server";

import {
  ACCESS_COOKIE,
  REFRESH_COOKIE,
  syncServerUrl,
} from "@/lib/server-api";
import {
  assertSameOrigin,
  clearLoginAttempts,
  getRequestIp,
  isLoginRateLimited,
  recordLoginAttempt,
} from "@/lib/security";

type LoginResponse = {
  access_token: string;
  refresh_token: string;
  expires_in: number;
};

type MeResponse = {
  role: string;
};

const secure = process.env.NODE_ENV === "production";

export async function POST(request: Request) {
  if (!assertSameOrigin(request)) {
    return NextResponse.json({ error: "Forbidden" }, { status: 403 });
  }

  const ip = getRequestIp(request);
  if (isLoginRateLimited(ip)) {
    return NextResponse.json(
      { error: "Too many login attempts" },
      { status: 429 },
    );
  }

  const body = (await request.json()) as {
    email?: string;
    password?: string;
    altcha_payload?: string;
  };
  const email = body.email?.trim().toLowerCase() ?? "";
  const password = body.password ?? "";
  const altchaPayload = body.altcha_payload?.trim() ?? undefined;

  if (!email || !password) {
    return NextResponse.json(
      { error: "Email and password are required" },
      { status: 400 },
    );
  }

  const loginRes = await fetch(syncServerUrl("/auth/login"), {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Accept: "application/json",
    },
    body: JSON.stringify({
      email,
      password,
      ...(altchaPayload ? { altcha_payload: altchaPayload } : {}),
    }),
    cache: "no-store",
  });

  if (!loginRes.ok) {
    recordLoginAttempt(ip);
    return NextResponse.json(
      { error: "Invalid credentials" },
      { status: loginRes.status === 401 ? 401 : 400 },
    );
  }

  const tokens = (await loginRes.json()) as LoginResponse;

  const meRes = await fetch(syncServerUrl("/auth/me"), {
    headers: {
      Authorization: `Bearer ${tokens.access_token}`,
      Accept: "application/json",
    },
    cache: "no-store",
  });

  if (!meRes.ok) {
    recordLoginAttempt(ip);
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const me = (await meRes.json()) as MeResponse;
  if (me.role !== "admin") {
    recordLoginAttempt(ip);
    return NextResponse.json({ error: "Forbidden" }, { status: 403 });
  }

  clearLoginAttempts(ip);

  const response = NextResponse.json({ ok: true });
  response.cookies.set(ACCESS_COOKIE, tokens.access_token, {
    httpOnly: true,
    sameSite: "strict",
    secure,
    path: "/",
    maxAge: tokens.expires_in,
  });
  response.cookies.set(REFRESH_COOKIE, tokens.refresh_token, {
    httpOnly: true,
    sameSite: "strict",
    secure,
    path: "/",
    maxAge: 60 * 60 * 24 * 30,
  });
  return response;
}
