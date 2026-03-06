import { NextResponse } from "next/server";

import { syncServerUrl } from "@/lib/server-api";
import { assertSameOrigin } from "@/lib/security";

type SetupAdminBody = {
  username?: string;
  email?: string;
  password?: string;
  setupToken?: string;
  altcha_payload?: string;
};

export async function POST(request: Request) {
  if (!assertSameOrigin(request)) {
    return NextResponse.json({ error: "Forbidden" }, { status: 403 });
  }

  const body = (await request.json()) as SetupAdminBody;
  const username = body.username?.trim() ?? "";
  const email = body.email?.trim().toLowerCase() ?? "";
  const password = body.password ?? "";
  const setupToken = body.setupToken?.trim() ?? "";
  const altchaPayload = body.altcha_payload?.trim() ?? undefined;

  if (!username || !email || password.length < 8 || !setupToken) {
    return NextResponse.json(
      {
        error:
          "username, email, setupToken required; password must be ≥8 chars",
      },
      { status: 400 },
    );
  }

  const response = await fetch(syncServerUrl("/auth/setup-admin"), {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Accept: "application/json",
    },
    body: JSON.stringify({
      username,
      email,
      password,
      setup_token: setupToken,
      ...(altchaPayload ? { altcha_payload: altchaPayload } : {}),
    }),
    cache: "no-store",
  });

  if (!response.ok) {
    const payload = (await response.json().catch(() => null)) as
      | { error?: string }
      | null;
    const fallback =
      response.status === 409
        ? "Admin account is already configured"
        : "Setup failed";

    return NextResponse.json(
      { error: payload?.error ?? fallback },
      { status: response.status },
    );
  }

  return NextResponse.json({ ok: true }, { status: 201 });
}
