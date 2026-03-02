const loginAttempts = new Map<string, number[]>();

const LOGIN_WINDOW_MS = 60_000;
const LOGIN_MAX_ATTEMPTS = 10;

function trimAttempts(now: number, attempts: number[]): number[] {
  return attempts.filter((ts) => now - ts <= LOGIN_WINDOW_MS);
}

export function getRequestIp(request: Request): string {
  const xff = request.headers.get("x-forwarded-for");
  if (xff) {
    return xff.split(",")[0].trim();
  }
  return "unknown";
}

export function isLoginRateLimited(ip: string): boolean {
  const now = Date.now();
  const current = trimAttempts(now, loginAttempts.get(ip) ?? []);
  loginAttempts.set(ip, current);
  return current.length >= LOGIN_MAX_ATTEMPTS;
}

export function recordLoginAttempt(ip: string): void {
  const now = Date.now();
  const current = trimAttempts(now, loginAttempts.get(ip) ?? []);
  current.push(now);
  loginAttempts.set(ip, current);
}

export function clearLoginAttempts(ip: string): void {
  loginAttempts.delete(ip);
}

export function assertSameOrigin(request: Request): boolean {
  const origin = request.headers.get("origin");
  const host = request.headers.get("host");
  if (!origin || !host) {
    return false;
  }

  try {
    const parsed = new URL(origin);
    return parsed.host === host;
  } catch {
    return false;
  }
}
