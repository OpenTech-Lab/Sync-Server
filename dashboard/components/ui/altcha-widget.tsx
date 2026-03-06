"use client";

import React, { useCallback, useEffect, useRef, useState } from "react";

type AltchaWidgetProps = {
  /**
   * Called with the base64-encoded verified payload, or `null` when ALTCHA is
   * not configured on the server and should be skipped.
   */
  onSolve: (payload: string | null) => void;
};

/**
 * Wraps the ALTCHA web component (`<altcha-widget>`).
 *
 * On mount the component probes `/api/altcha` (which proxies the backend
 * challenge endpoint).  If ALTCHA is disabled on the server (404), the widget
 * is never rendered and `onSolve(null)` is called immediately so the form can
 * proceed without a proof-of-work payload.  If ALTCHA is enabled the widget
 * is rendered and `onSolve` is called with the real payload once the user
 * solves the challenge.
 */
export function AltchaWidget({ onSolve }: AltchaWidgetProps) {
  const ref = useRef<HTMLElement>(null);
  const [enabled, setEnabled] = useState<boolean | null>(null);

  // Stable callback ref so the event-listener effect below does not re-run
  // on every render.
  const onSolveRef = useRef(onSolve);
  useEffect(() => {
    onSolveRef.current = onSolve;
  }, [onSolve]);

  // Probe the challenge endpoint once to decide whether to show the widget.
  useEffect(() => {
    let cancelled = false;

    fetch("/api/altcha", { cache: "no-store" })
      .then((res) => {
        if (cancelled) return;
        if (res.ok) {
          // ALTCHA is configured – load the web component and show the widget.
          void import("altcha");
          setEnabled(true);
        } else {
          // ALTCHA is disabled – skip silently.
          setEnabled(false);
          onSolveRef.current(null);
        }
      })
      .catch(() => {
        if (cancelled) return;
        // Network error – treat as disabled so login still works.
        setEnabled(false);
        onSolveRef.current(null);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  // Listen for the statechange event fired by the web component.
  const handleStateChange = useCallback((e: Event) => {
    const ev = e as AltchaStateChangeEvent;
    if (ev.detail.state === "verified" && ev.detail.payload) {
      onSolveRef.current(ev.detail.payload);
    }
  }, []);

  useEffect(() => {
    const el = ref.current;
    if (!el || !enabled) return;

    el.addEventListener("statechange", handleStateChange);
    return () => el.removeEventListener("statechange", handleStateChange);
  }, [enabled, handleStateChange]);

  if (!enabled) return null;

  return (
    <altcha-widget
      ref={ref}
      challengeurl="/api/altcha"
      hidefooter
    />
  );
}
