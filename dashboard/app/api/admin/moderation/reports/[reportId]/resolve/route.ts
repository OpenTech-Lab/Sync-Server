import { cookies } from "next/headers";
import { NextResponse } from "next/server";

import { ACCESS_COOKIE, syncServerUrl } from "@/lib/server-api";
import { assertSameOrigin } from "@/lib/security";

type Params = { params: Promise<{ reportId: string }> };

export async function POST(request: Request, { params }: Params) {
  if (!assertSameOrigin(request)) {
    return NextResponse.json({ error: "Forbidden" }, { status: 403 });
  }

  const { reportId } = await params;
  const jar = await cookies();
  const access = jar.get(ACCESS_COOKIE)?.value;
  if (!access) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const payload = await request.text();
  const response = await fetch(
    syncServerUrl(`/api/admin/moderation/reports/${reportId}/resolve`),
    {
      method: "POST",
      headers: {
        Authorization: `Bearer ${access}`,
        Accept: "application/json",
        "Content-Type": "application/json",
      },
      body: payload,
      cache: "no-store",
    },
  );

  const responseText = await response.text();
  if (!response.ok) {
    return NextResponse.json(
      { error: responseText || "Failed to resolve moderation report" },
      { status: 400 },
    );
  }

  return new NextResponse(responseText, {
    status: 200,
    headers: {
      "Content-Type": "application/json",
    },
  });
}
