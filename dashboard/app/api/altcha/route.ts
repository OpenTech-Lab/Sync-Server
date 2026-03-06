import { NextResponse } from "next/server";

import { syncServerUrl } from "@/lib/server-api";

/**
 * GET /api/altcha
 *
 * Proxies the ALTCHA challenge endpoint from the Sync backend so the browser
 * can fetch it without needing to know the server-side `SYNC_SERVER_URL`.
 *
 * Returns 404 when ALTCHA is not configured on the backend (used by the widget
 * to hide itself automatically).
 */
export async function GET() {
  try {
    const res = await fetch(syncServerUrl("/auth/altcha"), {
      cache: "no-store",
    });

    if (!res.ok) {
      // Propagate 404 (ALTCHA disabled) or any other error status.
      return new NextResponse(null, { status: res.status });
    }

    const data = await res.json();
    return NextResponse.json(data);
  } catch {
    return new NextResponse(null, { status: 502 });
  }
}
