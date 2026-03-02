import { cookies } from "next/headers";

export const ACCESS_COOKIE = "sync_dashboard_access_token";
export const REFRESH_COOKIE = "sync_dashboard_refresh_token";

const serverBaseUrl =
  process.env.SYNC_SERVER_URL?.trim() || "http://localhost:8080";

export function syncServerUrl(path: string): string {
  if (path.startsWith("http://") || path.startsWith("https://")) {
    return path;
  }
  return `${serverBaseUrl}${path.startsWith("/") ? "" : "/"}${path}`;
}

export async function authHeaders(): Promise<HeadersInit> {
  const jar = await cookies();
  const access = jar.get(ACCESS_COOKIE)?.value;
  if (!access) {
    return {};
  }
  return {
    Authorization: `Bearer ${access}`,
  };
}

export async function apiGetJson<T>(path: string): Promise<T> {
  const response = await fetch(syncServerUrl(path), {
    headers: {
      Accept: "application/json",
      ...(await authHeaders()),
    },
    cache: "no-store",
  });

  if (!response.ok) {
    throw new Error(`Request failed (${response.status})`);
  }
  return (await response.json()) as T;
}
