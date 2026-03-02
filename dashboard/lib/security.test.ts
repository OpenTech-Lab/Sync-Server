import {
  assertSameOrigin,
  clearLoginAttempts,
  getRequestIp,
  isLoginRateLimited,
  recordLoginAttempt,
} from "@/lib/security";

describe("security helpers", () => {
  it("extracts first forwarded ip", () => {
    const req = new Request("http://localhost/api", {
      headers: {
        "x-forwarded-for": "203.0.113.10, 10.0.0.2",
      },
    });
    expect(getRequestIp(req)).toBe("203.0.113.10");
  });

  it("validates same origin requests", () => {
    const good = new Request("http://localhost:3000/api", {
      method: "POST",
      headers: {
        origin: "http://localhost:3000",
        host: "localhost:3000",
      },
    });
    expect(assertSameOrigin(good)).toBe(true);

    const bad = new Request("http://localhost:3000/api", {
      method: "POST",
      headers: {
        origin: "https://evil.example",
        host: "localhost:3000",
      },
    });
    expect(assertSameOrigin(bad)).toBe(false);
  });

  it("applies login rate limit window", () => {
    const ip = "198.51.100.50";
    clearLoginAttempts(ip);

    for (let i = 0; i < 10; i += 1) {
      expect(isLoginRateLimited(ip)).toBe(false);
      recordLoginAttempt(ip);
    }

    expect(isLoginRateLimited(ip)).toBe(true);
    clearLoginAttempts(ip);
  });
});
