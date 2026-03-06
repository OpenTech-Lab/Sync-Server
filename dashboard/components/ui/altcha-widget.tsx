"use client";

import React, { useCallback, useEffect, useRef, useState } from "react";

type AltchaWidgetProps = {
  /**
   * Called with the base64-encoded verified payload, or `null` when ALTCHA is
   * not configured on the server and should be skipped.
   */
  onSolve: (payload: string | null) => void;
};

type ProbeStatus = "loading" | "enabled" | "disabled" | "error";
type ProbeResult = "enabled" | "not_found" | "error";
const ALTCHA_CANDIDATE_URLS = ["/api/altcha", "/auth/altcha"] as const;

/**
 * Wraps the ALTCHA web component (`<altcha-widget>`).
 *
 * On mount the component probes challenge endpoints in order:
 * `/api/altcha` (dashboard proxy) → `/auth/altcha` (direct same-origin backend).
 *
 * - **404 (confirmed twice)** → ALTCHA disabled; `onSolve(null)` is called so
 *   the form can proceed without a proof-of-work payload.
 * - **200** → ALTCHA enabled; the web component renders and auto-solves the
 *   PoW puzzle (`auto="onload"`) so no user interaction is required.
 * - **Any other status / network error** → shows an error state with a retry
 *   button.  We deliberately do NOT silently skip ALTCHA here because the
 *   backend may still require the payload, and proceeding without it would
 *   produce a confusing "Invalid credentials" error.
 */
export function AltchaWidget({ onSolve }: AltchaWidgetProps) {
  const ref = useRef<HTMLElement>(null);
  const [status, setStatus] = useState<ProbeStatus>("loading");
  const [challengeUrl, setChallengeUrl] = useState<
    (typeof ALTCHA_CANDIDATE_URLS)[number]
  >(ALTCHA_CANDIDATE_URLS[0]);

  // Stable callback ref so the event-listener effect below does not re-run
  // on every render.
  const onSolveRef = useRef(onSolve);
  useEffect(() => {
    onSolveRef.current = onSolve;
  }, [onSolve]);

  const probe = useCallback(() => {
    setStatus("loading");
    const probeRequest = (url: string) => fetch(url, { cache: "no-store" });
    const probeCandidate = async (url: string): Promise<ProbeResult> => {
      const first = await probeRequest(url);
      if (first.ok) {
        return "enabled";
      }
      if (first.status !== 404) {
        return "error";
      }
      // Confirm 404 once more before disabling this candidate so a transient
      // routing mismatch does not permanently skip verification.
      const second = await probeRequest(url);
      if (second.ok) {
        return "enabled";
      }
      return second.status === 404 ? "not_found" : "error";
    };

    void (async () => {
      let sawError = false;
      for (const url of ALTCHA_CANDIDATE_URLS) {
        try {
          const result = await probeCandidate(url);
          if (result === "enabled") {
            await import("altcha");
            setChallengeUrl(url);
            setStatus("enabled");
            return;
          }
          if (result === "error") {
            sawError = true;
          }
        } catch {
          sawError = true;
        }
      }

      if (sawError) {
        setStatus("error");
        return;
      }

      setStatus("disabled");
      onSolveRef.current(null);
    })();
  }, []);

  // Probe on mount.
  useEffect(() => {
    probe();
  }, [probe]);

  // Listen for the statechange event fired by the web component.
  const handleStateChange = useCallback((e: Event) => {
    const ev = e as AltchaStateChangeEvent;
    if (ev.detail.state === "verified" && ev.detail.payload) {
      onSolveRef.current(ev.detail.payload);
    }
  }, []);

  useEffect(() => {
    const el = ref.current;
    if (!el || status !== "enabled") return;

    el.addEventListener("statechange", handleStateChange);
    return () => el.removeEventListener("statechange", handleStateChange);
  }, [status, handleStateChange]);

  if (status === "disabled" || status === "loading") return null;

  if (status === "error") {
    return (
      <p className="text-sm text-destructive">
        Verification unavailable.{" "}
        <button
          type="button"
          className="underline"
          onClick={probe}
        >
          Retry
        </button>
      </p>
    );
  }

  return (
    <altcha-widget
      ref={ref}
      challengeurl={challengeUrl}
      auto="onload"
      hidefooter
    />
  );
}
