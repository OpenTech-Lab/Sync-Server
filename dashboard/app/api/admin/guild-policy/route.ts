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

export async function PUT(request: Request) {
  if (!assertSameOrigin(request)) {
    return NextResponse.json({ error: "Forbidden" }, { status: 403 });
  }

  const jar = await cookies();
  const access = jar.get(ACCESS_COOKIE)?.value;
  const refresh = jar.get(REFRESH_COOKIE)?.value;
  const payload = await request.json();

  let response: Response;
  if (access) {
    response = await fetch(syncServerUrl("/api/admin/guild-policy"), {
      method: "PUT",
      headers: {
        Authorization: `Bearer ${access}`,
        "Content-Type": "application/json",
        Accept: "application/json",
      },
      body: JSON.stringify(payload),
      cache: "no-store",
    });
  } else {
    response = new Response(null, { status: 401 });
  }

  if (response.status === 401 && refresh) {
    const refreshResponse = await fetch(syncServerUrl("/auth/refresh"), {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json",
      },
      body: JSON.stringify({ refresh_token: refresh }),
      cache: "no-store",
    });

    if (refreshResponse.ok) {
      const tokens = (await refreshResponse.json()) as RefreshResponse;
      response = await fetch(syncServerUrl("/api/admin/guild-policy"), {
        method: "PUT",
        headers: {
          Authorization: `Bearer ${tokens.access_token}`,
          "Content-Type": "application/json",
          Accept: "application/json",
        },
        body: JSON.stringify(payload),
        cache: "no-store",
      });

      if (response.ok) {
        const next = NextResponse.json(await response.json());
        next.cookies.set(ACCESS_COOKIE, tokens.access_token, {
          httpOnly: true,
          sameSite: "strict",
          secure,
          path: "/",
          maxAge: tokens.expires_in,
        });
        next.cookies.set(REFRESH_COOKIE, tokens.refresh_token, {
          httpOnly: true,
          sameSite: "strict",
          secure,
          path: "/",
          maxAge: 60 * 60 * 24 * 30,
        });
        return next;
      }
    }
  }

  if (!response.ok) {
    const body = await response.text();
    return NextResponse.json(
      { error: body || "Guild policy update failed" },
      { status: 400 },
    );
  }

  return NextResponse.json(await response.json());
}
