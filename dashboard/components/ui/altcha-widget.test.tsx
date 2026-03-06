import { render, screen, waitFor } from "@testing-library/react";
import { vi } from "vitest";

import { AltchaWidget } from "./altcha-widget";

function hexToArrayBuffer(hex: string): ArrayBuffer {
  return Uint8Array.from(
    hex.match(/.{1,2}/g)?.map((value) => parseInt(value, 16)) ?? [],
  ).buffer;
}

describe("AltchaWidget", () => {
  it("falls back to /auth/altcha and solves the challenge", async () => {
    const onSolve = vi.fn();
    const challenge = {
      algorithm: "SHA-256",
      challenge:
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      salt: "demo-salt?expires=1772786846&",
      signature:
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      maxnumber: 10,
    };

    vi.stubGlobal(
      "fetch",
      vi.fn().mockImplementation((url: string) => {
        if (url === "/api/altcha") {
          return Promise.resolve({ ok: false, status: 404 });
        }
        if (url === "/auth/altcha") {
          return Promise.resolve({
            ok: true,
            json: async () => challenge,
          });
        }
        throw new Error(`Unexpected URL: ${url}`);
      }),
    );
    vi.stubGlobal("btoa", (value: string) =>
      Buffer.from(value, "binary").toString("base64"),
    );
    vi.stubGlobal("crypto", {
      subtle: {
        digest: vi.fn(async (_algorithm: string, data: BufferSource) => {
          const bytes =
            data instanceof ArrayBuffer
              ? new Uint8Array(data)
              : new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
          const input = new TextDecoder().decode(bytes);
          return hexToArrayBuffer(
            input === `${challenge.salt}7`
              ? challenge.challenge
              : "00".repeat(32),
          );
        }),
      },
    });

    render(<AltchaWidget onSolve={onSolve} />);

    expect(screen.getByText("Preparing verification...")).toBeInTheDocument();

    await waitFor(() => {
      expect(onSolve).toHaveBeenCalledTimes(1);
    });

    const payload = onSolve.mock.calls[0]?.[0] as string;
    expect(typeof payload).toBe("string");

    const decoded = JSON.parse(
      Buffer.from(payload, "base64").toString("utf8"),
    ) as { number: number; challenge: string };
    expect(decoded.number).toBe(7);
    expect(decoded.challenge).toBe(challenge.challenge);

    vi.unstubAllGlobals();
  });
});
