"use client";

import Link from "next/link";
import { useCallback, useState } from "react";
import { useRouter } from "next/navigation";

import { Alert, AlertDescription } from "@/components/ui/alert";
import { AltchaWidget } from "@/components/ui/altcha-widget";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

export function LoginForm() {
  const router = useRouter();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  // null = ALTCHA still loading/pending; string = solved payload; empty
  // string means ALTCHA is disabled on the server (skip the check).
  const [altchaPayload, setAltchaPayload] = useState<string | null | undefined>(
    undefined,
  );

  const handleAltchaSolve = useCallback((payload: string | null) => {
    // null → ALTCHA disabled; forward an empty string so the backend ignores it.
    setAltchaPayload(payload ?? "");
  }, []);

  // The form is ready to submit once ALTCHA is either solved or disabled.
  const canSubmit = !submitting && altchaPayload !== undefined;

  async function onSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!canSubmit) return;
    setSubmitting(true);
    setError(null);

    try {
      const response = await fetch("./api/session/login", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          email,
          password,
          altcha_payload: altchaPayload || undefined,
        }),
      });

      if (!response.ok) {
        const body = (await response.json().catch(() => null)) as
          | { error?: string }
          | null;
        setError(body?.error ?? "Login failed");
        return;
      }

      router.push("/dashboard");
      router.refresh();
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <form className="space-y-4" onSubmit={onSubmit}>
      <div className="space-y-2">
        <Label htmlFor="login-email">Email</Label>
        <Input
          aria-label="Email"
          autoComplete="email"
          id="login-email"
          name="email"
          onChange={(event) => setEmail(event.target.value)}
          required
          type="email"
          value={email}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="login-password">Password</Label>
        <Input
          aria-label="Password"
          autoComplete="current-password"
          id="login-password"
          name="password"
          onChange={(event) => setPassword(event.target.value)}
          required
          type="password"
          value={password}
        />
      </div>

      <div className="text-right">
        <Link
          className="text-sm text-muted-foreground underline-offset-4 hover:underline"
          href="/forgot-password"
        >
          Forgot password?
        </Link>
      </div>

      {/* ALTCHA proof-of-work widget – hidden automatically when disabled */}
      <AltchaWidget onSolve={handleAltchaSolve} />

      {error ? (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      ) : null}

      <Button className="w-full" disabled={!canSubmit} type="submit">
        {submitting ? "Signing in..." : "Sign in"}
      </Button>
    </form>
  );
}
