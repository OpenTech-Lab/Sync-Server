import { cookies } from "next/headers";
import { NextResponse } from "next/server";

import { ACCESS_COOKIE, syncServerUrl } from "@/lib/server-api";
import { assertSameOrigin } from "@/lib/security";

type Params = { params: Promise<{ stickerId: string }> };

export async function GET(request: Request, { params }: Params) {
  if (!assertSameOrigin(request)) {
    return NextResponse.json({ error: "Forbidden" }, { status: 403 });
  }

  const { stickerId } = await params;
  const jar = await cookies();
  const access = jar.get(ACCESS_COOKIE)?.value;
  if (!access) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const response = await fetch(syncServerUrl(`/api/stickers/${stickerId}`), {
    headers: {
      Authorization: `Bearer ${access}`,
      Accept: "application/json",
    },
    cache: "no-store",
  });

  if (!response.ok) {
    return NextResponse.json({ error: "Not found" }, { status: 404 });
  }

  const data = (await response.json()) as {
    mime_type: string;
    content_base64: string;
  };

  const binaryString = atob(data.content_base64);
  const bytes = Uint8Array.from(binaryString, (c) => c.charCodeAt(0));

  return new NextResponse(bytes, {
    headers: {
      "Content-Type": data.mime_type,
      "Cache-Control": "public, max-age=86400",
    },
  });
}
