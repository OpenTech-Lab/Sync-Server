import { cookies } from "next/headers";
import { NextResponse } from "next/server";

import { ACCESS_COOKIE, syncServerUrl } from "@/lib/server-api";
import { assertSameOrigin } from "@/lib/security";

type Params = { params: Promise<{ stickerId: string }> };

export async function POST(request: Request, { params }: Params) {
  if (!assertSameOrigin(request)) {
    return NextResponse.json({ error: "Forbidden" }, { status: 403 });
  }

  const { stickerId } = await params;
  const jar = await cookies();
  const access = jar.get(ACCESS_COOKIE)?.value;
  if (!access) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const payload = await request.json();

  const response = await fetch(syncServerUrl(`/api/stickers/${stickerId}/moderate`), {
    method: "POST",
    headers: {
      Authorization: `Bearer ${access}`,
      "Content-Type": "application/json",
      Accept: "application/json",
    },
    body: JSON.stringify(payload),
    cache: "no-store",
  });

  if (!response.ok) {
    return NextResponse.json({ error: "Moderation failed" }, { status: 400 });
  }

  return NextResponse.json(await response.json());
}
