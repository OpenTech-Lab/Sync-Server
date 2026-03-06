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

/**
 * Wraps the ALTCHA web component (`<altcha-widget>`).
 *
 * On mount the component probes `/api/altcha` (which proxies the backend
 * challenge endpoint).
 *
 * - **404** → ALTCHA disabled; `onSolve(null)` is called immediately so the
 *   form can proceed without a proof-of-work payload.
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

  // Stable callback ref so the event-listener effect below does not re-run
  // on every render.
  const onSolveRef = useRef(onSolve);
  useEffect(() => {
    onSolveRef.current = onSolve;
  }, [onSolve]);

  const probe = useCallback(() => {
    setStatus("loading");
    fetch("/api/altcha", { cache: "no-store" })
      .then((res) => {
        if (res.status === 404) {
          // ALTCHA is not configured on this server – skip it.
          setStatus("disabled");
          onSolveRef.current(null);
        } else if (res.ok) {
          // ALTCHA is configured – load the web component and show the widget.
          void import("altcha");
          setStatus("enabled");
        } else {
          // Unexpected error (502, 500, …) – show retry so the user is not
          // silently locked out.
          setStatus("error");
        }
      })
      .catch(() => {
        setStatus("error");
      });
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
      challengeurl="/api/altcha"
      auto="onload"
      hidefooter
    />
  );
}
