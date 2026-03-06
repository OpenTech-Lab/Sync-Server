"use client";

import React, { useCallback, useEffect, useRef, useState } from "react";

type AltchaWidgetProps = {
  /**
   * Called with the base64-encoded verified payload, or `null` when ALTCHA is
   * not configured on the server and should be skipped.
   */
  onSolve: (payload: string | null) => void;
};

type ProbeStatus = "loading" | "verifying" | "disabled" | "solved" | "error";

type AltchaChallenge = {
  algorithm: string;
  challenge: string;
  salt: string;
  signature: string;
  maxnumber?: number;
  maxNumber?: number;
};

const ALTCHA_CANDIDATE_URLS = ["/api/altcha", "/auth/altcha"] as const;

function toHex(buffer: ArrayBuffer): string {
  return Array.from(new Uint8Array(buffer))
    .map((value) => value.toString(16).padStart(2, "0"))
    .join("");
}

function toBase64(value: string): string {
  return btoa(value);
}

async function solveAltchaChallenge(
  challenge: AltchaChallenge,
  signal: AbortSignal,
): Promise<string> {
  if (challenge.algorithm.toUpperCase() !== "SHA-256") {
    throw new Error(`Unsupported ALTCHA algorithm: ${challenge.algorithm}`);
  }
  if (
    typeof crypto === "undefined" ||
    !("subtle" in crypto) ||
    typeof crypto.subtle.digest !== "function"
  ) {
    throw new Error("Web Crypto is unavailable for ALTCHA verification.");
  }

  const encoder = new TextEncoder();
  const target = challenge.challenge.toLowerCase();
  const maxNumber = challenge.maxNumber ?? challenge.maxnumber ?? 1_000_000;
  const startedAt = Date.now();

  for (let number = 0; number <= maxNumber; number += 1) {
    if (signal.aborted) {
      throw new DOMException("ALTCHA verification aborted.", "AbortError");
    }

    const digest = await crypto.subtle.digest(
      "SHA-256",
      encoder.encode(`${challenge.salt}${number}`),
    );
    if (toHex(digest) !== target) {
      continue;
    }

    return toBase64(
      JSON.stringify({
        algorithm: challenge.algorithm,
        challenge: challenge.challenge,
        number,
        salt: challenge.salt,
        signature: challenge.signature,
        took: Date.now() - startedAt,
      }),
    );
  }

  throw new Error("Unable to solve the ALTCHA challenge.");
}

/**
 * Fetches + solves ALTCHA directly in React instead of relying on the
 * third-party web component runtime, which has been flaky in Firefox/Linux.
 */
export function AltchaWidget({ onSolve }: AltchaWidgetProps) {
  const onSolveRef = useRef(onSolve);
  const abortRef = useRef<AbortController | null>(null);
  const [status, setStatus] = useState<ProbeStatus>("loading");

  useEffect(() => {
    onSolveRef.current = onSolve;
  }, [onSolve]);

  useEffect(() => {
    return () => {
      abortRef.current?.abort();
    };
  }, []);

  const probe = useCallback(() => {
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    setStatus("loading");

    const fetchCandidate = async (
      url: (typeof ALTCHA_CANDIDATE_URLS)[number],
    ): Promise<AltchaChallenge | null> => {
      const request = () =>
        fetch(url, {
          cache: "no-store",
          headers: { Accept: "application/json" },
          signal: controller.signal,
        });

      const first = await request();
      if (first.ok) {
        return (await first.json()) as AltchaChallenge;
      }
      if (first.status !== 404) {
        throw new Error(`ALTCHA challenge request failed (${first.status}).`);
      }

      // Confirm 404 before treating this endpoint as unavailable so transient
      // proxy/router mismatches do not disable verification.
      const second = await request();
      if (second.ok) {
        return (await second.json()) as AltchaChallenge;
      }
      if (second.status === 404) {
        return null;
      }
      throw new Error(`ALTCHA challenge request failed (${second.status}).`);
    };

    void (async () => {
      let sawError = false;

      for (const url of ALTCHA_CANDIDATE_URLS) {
        try {
          const challenge = await fetchCandidate(url);
          if (challenge === null) {
            continue;
          }

          setStatus("verifying");
          const payload = await solveAltchaChallenge(challenge, controller.signal);
          if (controller.signal.aborted) {
            return;
          }

          setStatus("solved");
          onSolveRef.current(payload);
          return;
        } catch (error) {
          if (
            error instanceof DOMException &&
            error.name === "AbortError"
          ) {
            return;
          }
          sawError = true;
        }
      }

      if (controller.signal.aborted) {
        return;
      }

      if (sawError) {
        setStatus("error");
        return;
      }

      setStatus("disabled");
      onSolveRef.current(null);
    })();
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      probe();
    }, 0);
    return () => {
      window.clearTimeout(timer);
    };
  }, [probe]);

  if (status === "disabled" || status === "solved") {
    return null;
  }

  if (status === "error") {
    return (
      <p className="text-sm text-destructive">
        Verification unavailable.{" "}
        <button className="underline" onClick={probe} type="button">
          Retry
        </button>
      </p>
    );
  }

  return (
    <p className="text-sm text-muted-foreground">
      {status === "verifying" ? "Verifying..." : "Preparing verification..."}
    </p>
  );
}
