"use client";

import { useMemo, useState } from "react";
import { useRouter } from "next/navigation";

import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { Textarea } from "@/components/ui/textarea";

type TrustPolicyConfig = {
  enforcement: {
    enabled: boolean;
    outbound_messages_enabled: boolean;
    friend_adds_enabled: boolean;
    attachment_sends_enabled: boolean;
  };
  daily_counter_retention_days: number;
  score_event_retention_days: number;
  level_policies: Array<{
    level: number;
    min_active_days: number;
    max_active_days: number | null;
    daily_outbound_messages_limit: number | null;
    daily_friend_add_limit: number | null;
    daily_attachment_send_limit: number | null;
  }>;
  rank_policies: Array<{
    rank: string;
    min_score: number;
    max_score: number | null;
    daily_outbound_messages_limit_multiplier_percent: number | null;
    daily_friend_add_limit_multiplier_percent: number | null;
    daily_attachment_send_limit_multiplier_percent: number | null;
    overrides_level_limits: boolean;
  }>;
  community_upvote_daily_cap: number;
  safe_attachment_types: string[];
};

type TrustHistoryPruneResult = {
  daily_counter_retention_days: number;
  score_event_retention_days: number;
  pruned_before_day: string;
  pruned_before_timestamp: string;
  daily_action_counters_deleted: number;
  trust_score_events_deleted: number;
};

function formatPolicy(policy: TrustPolicyConfig) {
  return JSON.stringify(policy, null, 2);
}

function EnforcementBadge({
  label,
  enabled,
}: {
  label: string;
  enabled: boolean;
}) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-lg border p-3">
      <span className="text-sm text-muted-foreground">{label}</span>
      <Badge variant={enabled ? "default" : "secondary"}>
        {enabled ? "on" : "off"}
      </Badge>
    </div>
  );
}

export function TrustPolicyForm({ policy }: { policy: TrustPolicyConfig }) {
  const router = useRouter();
  const [draft, setDraft] = useState(() => formatPolicy(policy));
  const [saving, setSaving] = useState(false);
  const [pruning, setPruning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const parseResult = useMemo(() => {
    try {
      return {
        value: JSON.parse(draft) as TrustPolicyConfig,
        error: null,
      };
    } catch (parseError) {
      return {
        value: null,
        error: parseError instanceof Error ? parseError.message : "Invalid JSON",
      };
    }
  }, [draft]);

  const summaryPolicy = parseResult.value ?? policy;

  async function onSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);
    setNotice(null);

    if (parseResult.error || !parseResult.value) {
      setError(parseResult.error ?? "Trust policy JSON is invalid");
      return;
    }

    setSaving(true);
    const response = await fetch("/api/admin/trust-policy", {
      method: "PUT",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(parseResult.value),
    });

    if (!response.ok) {
      const body = (await response.json().catch(() => null)) as
        | { error?: string }
        | null;
      setError(body?.error ?? "Failed to save trust policy");
      setSaving(false);
      return;
    }

    const nextPolicy = (await response.json()) as TrustPolicyConfig;
    setDraft(formatPolicy(nextPolicy));
    setNotice("Trust policy updated.");
    setSaving(false);
    router.refresh();
  }

  async function pruneHistory() {
    if (
      !confirm(
        "Prune trust counters and score history using the current retention windows?",
      )
    ) {
      return;
    }

    setError(null);
    setNotice(null);
    setPruning(true);

    const response = await fetch("/api/admin/trust-policy/prune-history", {
      method: "POST",
    });

    if (!response.ok) {
      const body = (await response.json().catch(() => null)) as
        | { error?: string }
        | null;
      setError(body?.error ?? "Failed to prune trust history");
      setPruning(false);
      return;
    }

    const result = (await response.json()) as TrustHistoryPruneResult;
    setNotice(
      `Pruned ${result.daily_action_counters_deleted} daily counters and ${result.trust_score_events_deleted} score events.`,
    );
    setPruning(false);
    router.refresh();
  }

  function resetDraft() {
    setDraft(formatPolicy(policy));
    setError(null);
    setNotice(null);
  }

  function formatDraft() {
    if (!parseResult.value) {
      setError(parseResult.error ?? "Trust policy JSON is invalid");
      return;
    }
    setDraft(formatPolicy(parseResult.value));
    setError(null);
  }

  return (
    <form className="space-y-6" onSubmit={onSubmit}>
      <Separator />

      <section className="space-y-4">
        <div>
          <h2 className="text-xl font-semibold">Trust policy</h2>
          <p className="mt-1 text-sm text-muted-foreground">
            Manage trust caps, progression thresholds, enforcement switches, and
            attachment allowlists. The server validates and normalizes policy
            ranges before saving.
          </p>
        </div>

        <div className="grid gap-4 lg:grid-cols-3">
          <div className="space-y-3 rounded-lg border p-4">
            <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
              Enforcement
            </p>
            <div className="space-y-2">
              <EnforcementBadge
                enabled={summaryPolicy.enforcement.enabled}
                label="Global trust enforcement"
              />
              <EnforcementBadge
                enabled={summaryPolicy.enforcement.outbound_messages_enabled}
                label="Outbound messages"
              />
              <EnforcementBadge
                enabled={summaryPolicy.enforcement.friend_adds_enabled}
                label="Friend adds"
              />
              <EnforcementBadge
                enabled={summaryPolicy.enforcement.attachment_sends_enabled}
                label="Attachments"
              />
            </div>
          </div>

          <div className="space-y-3 rounded-lg border p-4">
            <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
              Thresholds
            </p>
            <div className="space-y-2 text-sm text-muted-foreground">
              <div className="flex items-center justify-between gap-3">
                <span>Level policies</span>
                <span className="font-medium text-foreground">
                  {summaryPolicy.level_policies.length}
                </span>
              </div>
              <div className="flex items-center justify-between gap-3">
                <span>Rank policies</span>
                <span className="font-medium text-foreground">
                  {summaryPolicy.rank_policies.length}
                </span>
              </div>
              <div className="flex items-center justify-between gap-3">
                <span>Upvote daily cap</span>
                <span className="font-medium text-foreground">
                  {summaryPolicy.community_upvote_daily_cap}
                </span>
              </div>
              <div className="flex items-center justify-between gap-3">
                <span>Counter retention</span>
                <span className="font-medium text-foreground">
                  {summaryPolicy.daily_counter_retention_days} days
                </span>
              </div>
              <div className="flex items-center justify-between gap-3">
                <span>Score retention</span>
                <span className="font-medium text-foreground">
                  {summaryPolicy.score_event_retention_days} days
                </span>
              </div>
            </div>
          </div>

          <div className="space-y-3 rounded-lg border p-4">
            <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
              Attachment allowlist
            </p>
            <div className="flex flex-wrap gap-2">
              {summaryPolicy.safe_attachment_types.map((entry) => (
                <Badge key={entry} variant="outline">
                  {entry}
                </Badge>
              ))}
              {summaryPolicy.safe_attachment_types.length === 0 ? (
                <span className="text-sm text-muted-foreground">
                  No attachment types allowed.
                </span>
              ) : null}
            </div>
          </div>
        </div>
      </section>

      <section className="space-y-3">
        <div className="space-y-2">
          <Label htmlFor="trust-policy-json">Trust policy JSON</Label>
          <Textarea
            className="min-h-[28rem] font-mono text-xs"
            id="trust-policy-json"
            onChange={(event) => setDraft(event.target.value)}
            value={draft}
          />
          <p className="text-xs text-muted-foreground">
            Edit the full policy directly to change caps, thresholds, or safe
            attachment types. Save will fail if ranges overlap or required
            fields are invalid.
          </p>
        </div>
      </section>

      {parseResult.error ? (
        <Alert variant="destructive">
          <AlertDescription>
            JSON parse error: {parseResult.error}
          </AlertDescription>
        </Alert>
      ) : null}

      {error ? (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      ) : null}

      {notice ? (
        <Alert>
          <AlertDescription>{notice}</AlertDescription>
        </Alert>
      ) : null}

      <div className="flex flex-wrap items-center gap-2">
        <Button onClick={formatDraft} type="button" variant="outline">
          Format JSON
        </Button>
        <Button onClick={resetDraft} type="button" variant="outline">
          Reset draft
        </Button>
        <Button
          disabled={pruning || saving}
          onClick={pruneHistory}
          type="button"
          variant="destructive"
        >
          {pruning ? "Pruning…" : "Prune retained history"}
        </Button>
        <div className="flex-1" />
        <Button disabled={saving || !!parseResult.error} type="submit">
          {saving ? "Saving…" : "Save trust policy"}
        </Button>
      </div>
    </form>
  );
}
