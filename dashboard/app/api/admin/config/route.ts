import { cookies } from "next/headers";
import { NextResponse } from "next/server";

import { ACCESS_COOKIE, syncServerUrl } from "@/lib/server-api";
import { assertSameOrigin } from "@/lib/security";

export async function PUT(request: Request) {
  if (!assertSameOrigin(request)) {
    return NextResponse.json({ error: "Forbidden" }, { status: 403 });
  }

  const jar = await cookies();
  const access = jar.get(ACCESS_COOKIE)?.value;
  if (!access) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const payload = await request.json();

  const response = await fetch(syncServerUrl("/api/admin/config"), {
    method: "PUT",
    headers: {
      Authorization: `Bearer ${access}`,
      "Content-Type": "application/json",
      Accept: "application/json",
    },
    body: JSON.stringify(payload),
    cache: "no-store",
  });

  if (!response.ok) {
    const body = await response.text();
    return NextResponse.json({ error: body || "Config update failed" }, { status: 400 });
  }

  return NextResponse.json(await response.json());
}
