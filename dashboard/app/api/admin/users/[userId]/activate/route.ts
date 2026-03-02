import { cookies } from "next/headers";
import { NextResponse } from "next/server";

import { ACCESS_COOKIE, syncServerUrl } from "@/lib/server-api";
import { assertSameOrigin } from "@/lib/security";

type Params = { params: Promise<{ userId: string }> };

export async function POST(request: Request, { params }: Params) {
  if (!assertSameOrigin(request)) {
    return NextResponse.json({ error: "Forbidden" }, { status: 403 });
  }

  const { userId } = await params;
  const jar = await cookies();
  const access = jar.get(ACCESS_COOKIE)?.value;
  if (!access) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const response = await fetch(
    syncServerUrl(`/api/admin/users/${userId}/activate`),
    {
      method: "POST",
      headers: {
        Authorization: `Bearer ${access}`,
        Accept: "application/json",
      },
      cache: "no-store",
    },
  );

  if (!response.ok) {
    return NextResponse.json({ error: "Failed to activate user" }, { status: 400 });
  }

  return NextResponse.json({ ok: true });
}
