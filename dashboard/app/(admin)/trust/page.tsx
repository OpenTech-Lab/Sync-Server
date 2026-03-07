import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { apiGetJson } from "@/lib/server-api";
import { requireAdminSession } from "@/lib/session";

import { TrustReviewUsersTable } from "./ui/trust-review-users-table";
import { TrustScoreEventsTable } from "./ui/trust-score-events-table";

type ReviewFilter = "flagged" | "challenged" | "frozen";

type AdminOverview = {
  system_status: string;
  total_users: number;
  active_users: number;
  admin_users: number;
  pending_approval: number;
  trust_challenged: number;
  trust_frozen: number;
  federation_pending: number;
  federation_failed: number;
  federation_dead_letter: number;
};

type AdminTrustReviewMetrics = {
  current_challenged_users: number;
  current_frozen_users: number;
  challenged_transitions: number;
  frozen_transitions: number;
  recovery_transitions: number;
  likely_false_positive_recoveries: number;
};

type AdminBlockedActionCount = {
  action: string;
  count: number;
};

type TrustScoreEventItem = {
  id: string;
  user_id: string;
  granter_user_id: string | null;
  event_type: string;
  delta: number;
  reference_id: string | null;
  metadata: Record<string, unknown>;
  created_at: string;
};

type TrustReviewUser = {
  id: string;
  username: string;
  email: string;
  role: string;
  is_active: boolean;
  is_approved: boolean;
  created_at: string;
  last_seen_at: string | null;
  trust: {
    active_days: number;
    derived_level: number;
    derived_rank: string;
    automation_review_state: string;
    suspicious_activity_streak: number;
    last_human_activity_at: string | null;
    last_active_day: string | null;
  } | null;
};

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
    overrides_level_limits: boolean;
  }>;
  community_upvote_daily_cap: number;
  safe_attachment_types: string[];
};

function normalizeFilter(value: string | undefined): ReviewFilter {
  if (value === "challenged" || value === "frozen") {
    return value;
  }
  return "flagged";
}

function filterHref(filter: ReviewFilter) {
  return filter === "flagged" ? "/trust" : `/trust?state=${filter}`;
}

function Stat({
  label,
  value,
  note,
}: {
  label: string;
  value: string | number;
  note?: string;
}) {
  return (
    <div className="bg-background px-5 py-4">
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className="mt-1 text-2xl font-semibold tabular-nums">{value}</p>
      {note ? <p className="mt-0.5 text-[11px] text-muted-foreground/70">{note}</p> : null}
    </div>
  );
}

export default async function TrustPage({
  searchParams,
}: {
  searchParams: Promise<{ state?: string }>;
}) {
  await requireAdminSession();
  const { state } = await searchParams;
  const reviewFilter = normalizeFilter(state);
  const reviewStateQuery =
    reviewFilter === "flagged" ? "" : `?automation_review_state=${reviewFilter}`;

  const [overview, metrics, blockedActions, scoreEvents, reviewUsers, trustPolicy] =
    await Promise.all([
      apiGetJson<AdminOverview>("/api/admin/overview"),
      apiGetJson<AdminTrustReviewMetrics>("/api/admin/trust-review-metrics"),
      apiGetJson<AdminBlockedActionCount[]>("/api/admin/trust-blocked-actions?limit=8"),
      apiGetJson<TrustScoreEventItem[]>("/api/admin/trust-score-events?limit=12"),
      apiGetJson<TrustReviewUser[]>(
        `/api/admin/trust-review-users${reviewStateQuery}${reviewStateQuery ? "&" : "?"}limit=25`,
      ),
      apiGetJson<TrustPolicyConfig>("/api/admin/trust-policy"),
    ]);

  return (
    <div className="space-y-8">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold">Trust Review</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Review suspicious automation, trust rollout state, blocked actions, and score activity.
          </p>
        </div>
        <Badge variant={trustPolicy.enforcement.enabled ? "default" : "secondary"}>
          Trust enforcement {trustPolicy.enforcement.enabled ? "enabled" : "disabled"}
        </Badge>
      </div>

      <section>
        <p className="mb-3 text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
          Challenge metrics
        </p>
        <dl className="grid grid-cols-2 gap-px overflow-hidden rounded-lg border bg-border lg:grid-cols-6">
          <Stat label="Challenged now" value={metrics.current_challenged_users} />
          <Stat label="Frozen now" value={metrics.current_frozen_users} />
          <Stat label="Challenge events" value={metrics.challenged_transitions} />
          <Stat label="Freeze events" value={metrics.frozen_transitions} />
          <Stat label="Recoveries" value={metrics.recovery_transitions} />
          <Stat
            label="Likely false positives"
            value={metrics.likely_false_positive_recoveries}
            note="Recovered from challenged/frozen"
          />
        </dl>
      </section>

      <section>
        <p className="mb-3 text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
          Rollout state
        </p>
        <div className="grid gap-4 lg:grid-cols-3">
          <Card>
            <CardHeader>
              <CardTitle>Live trust pressure</CardTitle>
              <CardDescription>Current flagged population across the instance.</CardDescription>
            </CardHeader>
            <CardContent className="space-y-2 text-sm text-muted-foreground">
              <div className="flex items-center justify-between">
                <span>Pending approvals</span>
                <span className="font-medium text-foreground">{overview.pending_approval}</span>
              </div>
              <div className="flex items-center justify-between">
                <span>Challenged users</span>
                <span className="font-medium text-foreground">{overview.trust_challenged}</span>
              </div>
              <div className="flex items-center justify-between">
                <span>Frozen users</span>
                <span className="font-medium text-foreground">{overview.trust_frozen}</span>
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Enforcement switches</CardTitle>
              <CardDescription>Server-side rollout state for each trust gate.</CardDescription>
            </CardHeader>
            <CardContent className="space-y-2 text-sm text-muted-foreground">
              <div className="flex items-center justify-between">
                <span>Outbound messages</span>
                <Badge variant={trustPolicy.enforcement.outbound_messages_enabled ? "default" : "secondary"}>
                  {trustPolicy.enforcement.outbound_messages_enabled ? "on" : "off"}
                </Badge>
              </div>
              <div className="flex items-center justify-between">
                <span>Friend adds</span>
                <Badge variant={trustPolicy.enforcement.friend_adds_enabled ? "default" : "secondary"}>
                  {trustPolicy.enforcement.friend_adds_enabled ? "on" : "off"}
                </Badge>
              </div>
              <div className="flex items-center justify-between">
                <span>Attachments</span>
                <Badge variant={trustPolicy.enforcement.attachment_sends_enabled ? "default" : "secondary"}>
                  {trustPolicy.enforcement.attachment_sends_enabled ? "on" : "off"}
                </Badge>
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Policy summary</CardTitle>
              <CardDescription>Retention and rule footprint for the current rollout.</CardDescription>
            </CardHeader>
            <CardContent className="space-y-2 text-sm text-muted-foreground">
              <div className="flex items-center justify-between">
                <span>Level policies</span>
                <span className="font-medium text-foreground">{trustPolicy.level_policies.length}</span>
              </div>
              <div className="flex items-center justify-between">
                <span>Rank policies</span>
                <span className="font-medium text-foreground">{trustPolicy.rank_policies.length}</span>
              </div>
              <div className="flex items-center justify-between">
                <span>Safe attachment types</span>
                <span className="font-medium text-foreground">{trustPolicy.safe_attachment_types.length}</span>
              </div>
              <div className="flex items-center justify-between">
                <span>Daily counter retention</span>
                <span className="font-medium text-foreground">
                  {trustPolicy.daily_counter_retention_days} days
                </span>
              </div>
              <div className="flex items-center justify-between">
                <span>Score event retention</span>
                <span className="font-medium text-foreground">
                  {trustPolicy.score_event_retention_days} days
                </span>
              </div>
            </CardContent>
          </Card>
        </div>
      </section>

      <section className="grid gap-4 xl:grid-cols-[minmax(0,2fr)_minmax(20rem,1fr)]">
        <div className="space-y-3">
          <div className="flex items-center justify-between gap-3">
            <div>
              <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
                Suspicious automation
              </p>
              <p className="mt-1 text-sm text-muted-foreground">
                Review users currently in challenged or frozen trust states.
              </p>
            </div>
            <div className="flex items-center gap-2">
              {(["flagged", "challenged", "frozen"] as const).map((item) => (
                <a
                  className={`rounded-md border px-2.5 py-1 text-xs font-medium transition-colors ${
                    reviewFilter === item
                      ? "border-foreground bg-foreground text-background"
                      : "border-border text-muted-foreground hover:text-foreground"
                  }`}
                  href={filterHref(item)}
                  key={item}
                >
                  {item}
                </a>
              ))}
            </div>
          </div>
          <TrustReviewUsersTable users={reviewUsers} />
        </div>

        <Card>
          <CardHeader>
            <CardTitle>Top blocked actions</CardTitle>
            <CardDescription>Most common trust rejections captured in admin audit logs.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {blockedActions.length === 0 ? (
              <p className="text-sm text-muted-foreground">No blocked trust actions recorded yet.</p>
            ) : (
              blockedActions.map((item) => (
                <div className="flex items-center justify-between gap-4" key={item.action}>
                  <span className="text-sm text-muted-foreground">{item.action}</span>
                  <Badge variant="secondary">{item.count}</Badge>
                </div>
              ))
            )}
          </CardContent>
        </Card>
      </section>

      <section>
        <p className="mb-3 text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
          Score-affecting events
        </p>
        <TrustScoreEventsTable events={scoreEvents} />
      </section>
    </div>
  );
}
