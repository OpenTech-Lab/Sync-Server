import { NextResponse } from "next/server";

import { syncServerUrl } from "@/lib/server-api";
import { assertSameOrigin } from "@/lib/security";

const SUCCESS_MESSAGE = "If that email is registered, a reset link was sent.";

export async function POST(request: Request) {
  if (!assertSameOrigin(request)) {
    return NextResponse.json({ error: "Forbidden" }, { status: 403 });
  }

  const body = (await request.json().catch(() => null)) as
    | { email?: string }
    | null;
  const email = body?.email?.trim().toLowerCase() ?? "";

  if (!email) {
    return NextResponse.json({ error: "Email is required" }, { status: 400 });
  }

  const response = await fetch(syncServerUrl("/auth/forgot-password"), {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Accept: "application/json",
    },
    body: JSON.stringify({ email }),
    cache: "no-store",
  });

  if (!response.ok) {
    return NextResponse.json(
      { error: "Could not process request" },
      { status: 502 },
    );
  }

  return NextResponse.json({ message: SUCCESS_MESSAGE });
}
