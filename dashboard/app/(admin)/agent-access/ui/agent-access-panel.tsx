"use client";

import { useState } from "react";

import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";

export type AgentTokenView = {
  id: string;
  name: string;
  token_prefix: string;
  created_at: string;
  expires_at: string | null;
  last_used_at: string | null;
  revoked_at: string | null;
};

type CreatedToken = {
  id: string;
  name: string;
  token: string;
  token_prefix: string;
  created_at: string;
  expires_at: string | null;
};

const EXPIRY_OPTIONS = [
  { value: "", label: "Never" },
  { value: "30", label: "30 days" },
  { value: "90", label: "90 days" },
];

function tokenStatus(token: AgentTokenView): { label: string; variant: "outline" | "destructive" | "secondary" } {
  if (token.revoked_at) {
    return { label: "Revoked", variant: "destructive" };
  }
  if (token.expires_at && new Date(token.expires_at).getTime() <= Date.now()) {
    return { label: "Expired", variant: "secondary" };
  }
  return { label: "Active", variant: "outline" };
}

export function AgentAccessPanel({ initialTokens }: { initialTokens: AgentTokenView[] }) {
  const [name, setName] = useState("");
  const [expiryDays, setExpiryDays] = useState("");
  const [items, setItems] = useState(initialTokens);
  const [creating, setCreating] = useState(false);
  const [revokingId, setRevokingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [justCreated, setJustCreated] = useState<CreatedToken | null>(null);
  const [copied, setCopied] = useState(false);

  async function onSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setCreating(true);
    setError(null);
    setCopied(false);

    try {
      const response = await fetch("/api/admin/agent-tokens", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name,
          expires_in_days: expiryDays ? Number(expiryDays) : null,
        }),
      });

      if (!response.ok) {
        const body = (await response.json().catch(() => null)) as { error?: string } | null;
        setError(body?.error ?? "Failed to create agent token");
        return;
      }

      const created = (await response.json()) as CreatedToken;
      setJustCreated(created);
      setItems((prev) => [
        {
          id: created.id,
          name: created.name,
          token_prefix: created.token_prefix,
          created_at: created.created_at,
          expires_at: created.expires_at,
          last_used_at: null,
          revoked_at: null,
        },
        ...prev,
      ]);
      setName("");
      setExpiryDays("");
    } catch {
      setError("Failed to create agent token");
    } finally {
      setCreating(false);
    }
  }

  async function onRevoke(token: AgentTokenView) {
    if (!confirm(`Revoke "${token.name}"? Any agent using it will immediately lose access.`)) {
      return;
    }
    setRevokingId(token.id);
    setError(null);

    try {
      const response = await fetch(`/api/admin/agent-tokens/${token.id}`, {
        method: "DELETE",
      });

      if (!response.ok) {
        const body = (await response.json().catch(() => null)) as { error?: string } | null;
        setError(body?.error ?? "Failed to revoke agent token");
        return;
      }

      setItems((prev) =>
        prev.map((item) =>
          item.id === token.id ? { ...item, revoked_at: new Date().toISOString() } : item,
        ),
      );
    } catch {
      setError("Failed to revoke agent token");
    } finally {
      setRevokingId(null);
    }
  }

  async function copyToken() {
    if (!justCreated) {
      return;
    }
    await navigator.clipboard.writeText(justCreated.token);
    setCopied(true);
  }

  return (
    <div className="space-y-8">
      {justCreated ? (
        <Alert>
          <AlertDescription className="space-y-3">
            <p className="font-medium text-foreground">
              Token created for &ldquo;{justCreated.name}&rdquo;. Copy it now — it will not be
              shown again.
            </p>
            <div className="flex flex-wrap items-center gap-2">
              <code className="rounded bg-muted px-2 py-1 text-xs">{justCreated.token}</code>
              <Button onClick={() => void copyToken()} size="sm" type="button" variant="outline">
                {copied ? "Copied" : "Copy"}
              </Button>
              <Button onClick={() => setJustCreated(null)} size="sm" type="button" variant="ghost">
                Dismiss
              </Button>
            </div>
            <p className="text-xs text-muted-foreground">
              Have the agent send this in every admin API request as{" "}
              <code className="rounded bg-muted px-1">Authorization: Bearer {justCreated.token_prefix}…</code>
            </p>
          </AlertDescription>
        </Alert>
      ) : null}

      <section className="space-y-4">
        <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
          New agent token
        </p>
        <form className="space-y-4" onSubmit={onSubmit}>
          <div className="grid gap-4 sm:grid-cols-2">
            <div className="space-y-2">
              <Label htmlFor="agent-token-name">Name</Label>
              <Input
                id="agent-token-name"
                maxLength={120}
                onChange={(event) => setName(event.target.value)}
                placeholder="Claude ops agent"
                required
                type="text"
                value={name}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="agent-token-expiry">Expires</Label>
              <select
                className="border-input flex h-9 w-full rounded-md border bg-transparent px-3 py-1 text-sm shadow-xs"
                id="agent-token-expiry"
                onChange={(event) => setExpiryDays(event.target.value)}
                value={expiryDays}
              >
                {EXPIRY_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </div>
          </div>

          {error ? (
            <Alert variant="destructive">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          ) : null}

          <Button disabled={creating} type="submit">
            {creating ? "Creating…" : "Create token"}
          </Button>
        </form>
      </section>

      <Separator />

      <section className="space-y-4">
        <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
          Tokens <span className="normal-case font-normal text-muted-foreground/50">({items.length})</span>
        </p>

        {items.length === 0 ? (
          <p className="text-sm text-muted-foreground">No agent tokens issued yet.</p>
        ) : (
          <div className="divide-y rounded-lg border">
            {items.map((item) => {
              const status = tokenStatus(item);
              const isRevoked = Boolean(item.revoked_at);
              return (
                <div className="flex items-center justify-between gap-4 px-4 py-3" key={item.id}>
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <p className="font-medium leading-tight">{item.name}</p>
                      <Badge variant={status.variant}>{status.label}</Badge>
                    </div>
                    <p className="mt-1 text-xs text-muted-foreground/60">
                      {item.token_prefix}… · created {new Date(item.created_at).toLocaleString()}
                      {item.expires_at ? ` · expires ${new Date(item.expires_at).toLocaleString()}` : ""}
                      {item.last_used_at ? ` · last used ${new Date(item.last_used_at).toLocaleString()}` : ""}
                    </p>
                  </div>
                  <Button
                    disabled={isRevoked || revokingId === item.id}
                    onClick={() => void onRevoke(item)}
                    size="sm"
                    type="button"
                    variant="destructive"
                  >
                    {revokingId === item.id ? "Revoking…" : isRevoked ? "Revoked" : "Revoke"}
                  </Button>
                </div>
              );
            })}
          </div>
        )}
      </section>
    </div>
  );
}
