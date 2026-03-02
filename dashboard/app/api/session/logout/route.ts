import { cookies } from "next/headers";
import { NextResponse } from "next/server";

import { ACCESS_COOKIE, REFRESH_COOKIE, syncServerUrl } from "@/lib/server-api";
import { assertSameOrigin } from "@/lib/security";

export async function POST(request: Request) {
  if (!assertSameOrigin(request)) {
    return NextResponse.json({ error: "Forbidden" }, { status: 403 });
  }

  const jar = await cookies();
  const refresh = jar.get(REFRESH_COOKIE)?.value;

  if (refresh) {
    await fetch(syncServerUrl("/auth/logout"), {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json",
      },
      body: JSON.stringify({ refresh_token: refresh }),
      cache: "no-store",
    });
  }

  const response = NextResponse.json({ ok: true });
  response.cookies.delete(ACCESS_COOKIE);
  response.cookies.delete(REFRESH_COOKIE);
  return response;
}
