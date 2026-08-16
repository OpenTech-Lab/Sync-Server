// next.config.ts's basePath ("/admin"). fetch() is not basePath-aware like
// next/link, so client-side calls to our own API routes must prefix it
// manually or they 404 against the un-prefixed path.
const BASE_PATH = "/admin";

export function apiUrl(path: string): string {
  return `${BASE_PATH}${path.startsWith("/") ? "" : "/"}${path}`;
}
