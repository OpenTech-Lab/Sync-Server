"use client";

import Link from "next/link";
import { useEffect, useState } from "react";

const SUCCESS_MESSAGE = "If that email is registered, a reset link was sent.";
const RESEND_COOLDOWN_SECONDS = 60;

export function ForgotPasswordForm() {
  const [email, setEmail] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [cooldownRemaining, setCooldownRemaining] = useState(0);

  useEffect(() => {
    if (cooldownRemaining <= 0) {
      return;
    }

    const timer = window.setInterval(() => {
      setCooldownRemaining((seconds) => Math.max(seconds - 1, 0));
    }, 1000);

    return () => window.clearInterval(timer);
  }, [cooldownRemaining]);

  async function onSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSubmitting(true);
    setError(null);
    setMessage(null);

    try {
      const response = await fetch("/api/session/forgot-password", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ email }),
      });

      if (!response.ok) {
        const body = (await response.json().catch(() => null)) as
          | { error?: string }
          | null;
        setError(body?.error ?? "Request failed");
        return;
      }

      setMessage(SUCCESS_MESSAGE);
      setCooldownRemaining(RESEND_COOLDOWN_SECONDS);
    } finally {
      setSubmitting(false);
    }
  }

  const isSubmitDisabled = submitting || cooldownRemaining > 0;
  const submitLabel = submitting
    ? "Sending reset link..."
    : cooldownRemaining > 0
      ? `Send reset link (${cooldownRemaining}s)`
      : "Send reset link";

  return (
    <form className="mt-6 space-y-4" onSubmit={onSubmit}>
      <label className="block text-sm">
        <span className="mb-1 block text-muted-foreground">Email</span>
        <input
          aria-label="Email"
          autoComplete="email"
          className="w-full rounded-md border bg-background px-3 py-2"
          name="email"
          onChange={(event) => setEmail(event.target.value)}
          required
          type="email"
          value={email}
        />
      </label>
      {error ? <p className="text-sm text-destructive">{error}</p> : null}
      {message ? <p className="text-sm text-emerald-600">{message}</p> : null}
      <button
        className="w-full rounded-md bg-primary px-3 py-2 text-primary-foreground disabled:opacity-70"
        disabled={isSubmitDisabled}
        type="submit"
      >
        {submitLabel}
      </button>
      <div className="text-center">
        <Link
          className="text-sm text-muted-foreground underline-offset-4 hover:underline"
          href="/login"
        >
          Back to login
        </Link>
      </div>
    </form>
  );
}
