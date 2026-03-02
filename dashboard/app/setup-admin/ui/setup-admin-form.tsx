"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";

export function SetupAdminForm() {
  const router = useRouter();
  const [username, setUsername] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  async function onSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSubmitting(true);
    setError(null);

    if (password !== confirmPassword) {
      setError("Passwords do not match");
      setSubmitting(false);
      return;
    }

    try {
      const response = await fetch("/api/session/setup-admin", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ username, email, password }),
      });

      if (!response.ok) {
        const body = (await response.json().catch(() => null)) as
          | { error?: string }
          | null;
        setError(body?.error ?? "Setup failed");
        return;
      }

      router.push("/login");
      router.refresh();
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <form className="mt-6 space-y-4" onSubmit={onSubmit}>
      <label className="block text-sm">
        <span className="mb-1 block text-muted-foreground">Username</span>
        <input
          aria-label="Username"
          autoComplete="username"
          className="w-full rounded-md border bg-background px-3 py-2"
          name="username"
          onChange={(event) => setUsername(event.target.value)}
          required
          value={username}
        />
      </label>
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
      <label className="block text-sm">
        <span className="mb-1 block text-muted-foreground">Password</span>
        <input
          aria-label="Password"
          autoComplete="new-password"
          className="w-full rounded-md border bg-background px-3 py-2"
          minLength={8}
          name="password"
          onChange={(event) => setPassword(event.target.value)}
          required
          type="password"
          value={password}
        />
      </label>
      <label className="block text-sm">
        <span className="mb-1 block text-muted-foreground">Confirm Password</span>
        <input
          aria-label="Confirm Password"
          autoComplete="new-password"
          className="w-full rounded-md border bg-background px-3 py-2"
          minLength={8}
          name="confirm-password"
          onChange={(event) => setConfirmPassword(event.target.value)}
          required
          type="password"
          value={confirmPassword}
        />
      </label>
      {error ? <p className="text-sm text-destructive">{error}</p> : null}
      <button
        className="w-full rounded-md bg-primary px-3 py-2 text-primary-foreground disabled:opacity-70"
        disabled={submitting}
        type="submit"
      >
        {submitting ? "Creating account..." : "Create admin account"}
      </button>
    </form>
  );
}
