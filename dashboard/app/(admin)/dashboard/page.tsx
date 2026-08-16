import Link from "next/link";
import type { LucideIcon } from "lucide-react";
import {
  Activity,
  ArrowUpRight,
  Bot,
  Clock3,
  Network,
  ShieldAlert,
  ShieldCheck,
  Users,
} from "lucide-react";

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

type Overview = {
  system_status: string;
  total_users: number;
  active_users: number;
  admin_users: number;
  pending_approval: number;
  guild_challenged: number;
  guild_frozen: number;
  federation_pending: number;
  federation_failed: number;
  federation_dead_letter: number;
};

type MetricTone = "neutral" | "positive" | "warning" | "danger";

const METRIC_TONES: Record<MetricTone, { icon: string; value: string }> = {
  neutral: {
    icon: "bg-muted text-muted-foreground",
    value: "text-foreground",
  },
  positive: {
    icon: "bg-success/10 text-success",
    value: "text-success",
  },
  warning: {
    icon: "bg-warning/10 text-warning",
    value: "text-warning",
  },
  danger: {
    icon: "bg-destructive/10 text-destructive",
    value: "text-destructive",
  },
};

function MetricCard({
  detail,
  icon: Icon,
  label,
  tone,
  value,
}: {
  detail: string;
  icon: LucideIcon;
  label: string;
  tone: MetricTone;
  value: number;
}) {
  const colors = METRIC_TONES[tone];

  return (
    <Card className="gap-0 py-0 shadow-none transition-shadow hover:shadow-sm">
      <CardContent className="p-5">
        <div className="flex items-start justify-between gap-3">
          <div className={`flex size-9 items-center justify-center rounded-xl ${colors.icon}`}>
            <Icon aria-hidden="true" className="size-4" />
          </div>
          <span className="pt-1 text-right text-[11px] font-medium text-muted-foreground">{label}</span>
        </div>
        <p className={`mt-4 text-3xl font-semibold tracking-tight tabular-nums ${colors.value}`}>
          {value}
        </p>
        <p className="mt-1 text-xs text-muted-foreground">{detail}</p>
      </CardContent>
    </Card>
  );
}

function AttentionRow({
  detail,
  href,
  icon: Icon,
  label,
  tone,
  value,
}: {
  detail: string;
  href: string;
  icon: LucideIcon;
  label: string;
  tone: MetricTone;
  value: number;
}) {
  const colors = METRIC_TONES[tone];
  const hasItems = value > 0;

  return (
    <Link
      className="group flex items-center gap-3 rounded-xl border border-transparent px-2.5 py-3 transition-colors hover:border-border hover:bg-muted/50"
      href={href}
    >
      <span className={`flex size-8 shrink-0 items-center justify-center rounded-lg ${colors.icon}`}>
        <Icon aria-hidden="true" className="size-4" />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-sm font-medium">{label}</span>
        <span className="mt-0.5 block truncate text-xs text-muted-foreground">{detail}</span>
      </span>
      <span className="flex shrink-0 items-center gap-2">
        <Badge className="tabular-nums" variant={hasItems ? "secondary" : "outline"}>
          {hasItems ? value : "Clear"}
        </Badge>
        <ArrowUpRight
          aria-hidden="true"
          className="size-3.5 text-muted-foreground transition-transform group-hover:-translate-y-0.5 group-hover:translate-x-0.5 group-hover:text-foreground"
        />
      </span>
    </Link>
  );
}

function FederationRow({
  color,
  detail,
  label,
  value,
}: {
  color: string;
  detail: string;
  label: string;
  value: number;
}) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="flex min-w-0 items-center gap-2.5">
        <span aria-hidden="true" className={`size-2 rounded-full ${color}`} />
        <span className="truncate text-sm">{label}</span>
        <span className="hidden text-xs text-muted-foreground sm:inline">{detail}</span>
      </div>
      <span className="text-sm font-semibold tabular-nums">{value}</span>
    </div>
  );
}

export default async function DashboardPage() {
  await requireAdminSession();
  const overview = await apiGetJson<Overview>("/api/admin/overview");
  const statusOk = overview.system_status.toLowerCase() === "ok";
  const activeRate = overview.total_users
    ? Math.round((overview.active_users / overview.total_users) * 100)
    : 0;
  const guildReviewCount = overview.guild_challenged + overview.guild_frozen;
  const federationIssueCount = overview.federation_failed + overview.federation_dead_letter;
  const federationTotal =
    overview.federation_pending + federationIssueCount;
  const federationScale = Math.max(federationTotal, 1);

  return (
    <div className="space-y-8">
      <section className="relative isolate overflow-hidden rounded-2xl border border-hero-foreground/10 bg-hero px-5 py-6 text-hero-foreground shadow-sm sm:px-7 sm:py-8">
        <div
          aria-hidden="true"
          className="absolute -top-28 -right-20 -z-10 size-80 rounded-full bg-gradient-to-br from-emerald-300/50 via-cyan-300/25 to-blue-500/20 blur-3xl"
        />
        <div
          aria-hidden="true"
          className="absolute right-0 bottom-0 -z-10 h-full w-1/2 opacity-20 [background-image:linear-gradient(rgba(255,255,255,.18)_1px,transparent_1px),linear-gradient(90deg,rgba(255,255,255,.18)_1px,transparent_1px)] [background-size:28px_28px] [mask-image:linear-gradient(to_left,black,transparent)]"
        />
        <div className="relative flex flex-col gap-7 sm:flex-row sm:items-end sm:justify-between">
          <div className="max-w-2xl">
            <div className="mb-4 flex flex-wrap items-center gap-2 text-[10px] font-semibold uppercase tracking-[0.2em] text-hero-foreground/60">
              <span>Sync / Admin console</span>
              <span aria-hidden="true" className="size-1 rounded-full bg-hero-foreground/40" />
              <span>Live instance signal</span>
            </div>
            <h1 className="text-3xl font-semibold tracking-tight sm:text-4xl">System overview</h1>
            <p className="mt-3 max-w-xl text-sm leading-6 text-hero-foreground/70 sm:text-base">
              Keep an eye on account activity, trust review, and federation delivery from one control room.
            </p>
          </div>

          <div className="w-full shrink-0 rounded-2xl border border-hero-foreground/15 bg-hero-foreground/10 p-4 sm:w-52">
            <div className="flex items-center justify-between gap-3">
              <span className="text-xs font-medium text-hero-foreground/65">Instance status</span>
              <Activity aria-hidden="true" className="size-4 text-success" />
            </div>
            <div className="mt-4 flex items-center gap-2.5">
              <span
                aria-hidden="true"
                className={`size-2.5 rounded-full ring-4 ${statusOk ? "bg-success ring-success/15" : "bg-warning ring-warning/15"}`}
              />
              <span className="text-lg font-semibold">{statusOk ? "Operational" : "Needs attention"}</span>
            </div>
            <Badge className="mt-3 border-hero-foreground/15 bg-hero-foreground/10 text-hero-foreground uppercase tracking-wide" variant="outline">
              {overview.system_status}
            </Badge>
          </div>
        </div>
      </section>

      <section aria-labelledby="pulse-heading" className="space-y-4">
        <div className="flex flex-wrap items-end justify-between gap-3">
          <div>
            <p className="text-[10px] font-semibold uppercase tracking-[0.2em] text-muted-foreground/70">Current signal</p>
            <h2 className="mt-1 text-lg font-semibold tracking-tight" id="pulse-heading">Operational pulse</h2>
          </div>
          <span className="text-xs text-muted-foreground">{activeRate}% of accounts are active</span>
        </div>
        <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
          <MetricCard
            detail={`${activeRate}% active accounts`}
            icon={Users}
            label="Total users"
            tone="neutral"
            value={overview.total_users}
          />
          <MetricCard
            detail="Seen in the active window"
            icon={Activity}
            label="Active users"
            tone="positive"
            value={overview.active_users}
          />
          <MetricCard
            detail={overview.pending_approval > 0 ? "Waiting for approval" : "No approval backlog"}
            icon={Clock3}
            label="Pending approval"
            tone={overview.pending_approval > 0 ? "warning" : "positive"}
            value={overview.pending_approval}
          />
          <MetricCard
            detail={`${overview.guild_challenged} challenged · ${overview.guild_frozen} frozen`}
            icon={ShieldAlert}
            label="Guild review"
            tone={guildReviewCount > 0 ? "warning" : "positive"}
            value={guildReviewCount}
          />
        </div>
      </section>

      <section className="grid gap-4 xl:grid-cols-[minmax(0,1.08fr)_minmax(22rem,0.92fr)]">
        <Card>
          <CardHeader className="flex flex-row items-start justify-between gap-4 space-y-0">
            <div className="flex items-start gap-3">
              <span className="flex size-9 items-center justify-center rounded-xl bg-warning/10 text-warning">
                <ShieldCheck aria-hidden="true" className="size-4" />
              </span>
              <div>
                <CardTitle>Attention lanes</CardTitle>
                <CardDescription className="mt-1">Queues that may need an operator next.</CardDescription>
              </div>
            </div>
            {pendingApprovalOrReview(overview) ? (
              <Badge variant="secondary">Review open</Badge>
            ) : (
              <Badge variant="outline">All clear</Badge>
            )}
          </CardHeader>
          <CardContent className="space-y-1 px-5 pb-5 sm:px-6">
            <AttentionRow
              detail="Users waiting for access to the instance"
              href="/users?pending=1"
              icon={Clock3}
              label="Pending approvals"
              tone="warning"
              value={overview.pending_approval}
            />
            <AttentionRow
              detail="Accounts flagged for an automation challenge"
              href="/guild?state=challenged"
              icon={ShieldAlert}
              label="Challenged users"
              tone="warning"
              value={overview.guild_challenged}
            />
            <AttentionRow
              detail="Progression currently paused for review"
              href="/guild?state=frozen"
              icon={ShieldCheck}
              label="Frozen users"
              tone="danger"
              value={overview.guild_frozen}
            />
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-start justify-between gap-4 space-y-0">
            <div className="flex items-start gap-3">
              <span className="flex size-9 items-center justify-center rounded-xl bg-info/10 text-info">
                <Network aria-hidden="true" className="size-4" />
              </span>
              <div>
                <CardTitle>Federation queue</CardTitle>
                <CardDescription className="mt-1">Outbound delivery across connected instances.</CardDescription>
              </div>
            </div>
            <Badge variant={federationIssueCount > 0 ? "secondary" : "outline"}>
              {federationIssueCount > 0 ? "Review" : "Stable"}
            </Badge>
          </CardHeader>
          <CardContent className="space-y-5 px-5 pb-5 sm:px-6">
            <div aria-label="Federation queue breakdown" className="flex h-2 overflow-hidden rounded-full bg-muted">
              <span
                className="bg-info transition-all"
                style={{ width: `${(overview.federation_pending / federationScale) * 100}%` }}
              />
              <span
                className="bg-warning transition-all"
                style={{ width: `${(overview.federation_failed / federationScale) * 100}%` }}
              />
              <span
                className="bg-destructive transition-all"
                style={{ width: `${(overview.federation_dead_letter / federationScale) * 100}%` }}
              />
            </div>
            <div className="space-y-4">
              <FederationRow
                color="bg-info"
                detail="Waiting to send"
                label="Queued outbound"
                value={overview.federation_pending}
              />
              <FederationRow
                color="bg-warning"
                detail="Retryable delivery errors"
                label="Failed deliveries"
                value={overview.federation_failed}
              />
              <FederationRow
                color="bg-destructive"
                detail="Needs manual inspection"
                label="Dead-letter"
                value={overview.federation_dead_letter}
              />
            </div>
            <div className="flex items-center justify-between border-t pt-4 text-xs">
              <span className="text-muted-foreground">Total messages in queue</span>
              <span className="font-semibold tabular-nums">{federationTotal}</span>
            </div>
          </CardContent>
        </Card>
      </section>

      <section className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader className="flex flex-row items-start gap-3 space-y-0">
            <span className="flex size-9 items-center justify-center rounded-xl bg-muted text-muted-foreground">
              <Users aria-hidden="true" className="size-4" />
            </span>
            <div>
              <CardTitle>Account posture</CardTitle>
              <CardDescription className="mt-1">A quick read on the instance’s user base.</CardDescription>
            </div>
          </CardHeader>
          <CardContent className="space-y-5 px-5 pb-5 sm:px-6">
            <div>
              <div className="mb-2 flex items-center justify-between gap-3 text-sm">
                <span>Active accounts</span>
                <span className="font-medium tabular-nums text-muted-foreground">
                  {overview.active_users} <span className="font-normal">/ {overview.total_users}</span>
                </span>
              </div>
              <div aria-hidden="true" className="h-2 overflow-hidden rounded-full bg-muted">
                <div className="h-full rounded-full bg-success transition-all" style={{ width: `${activeRate}%` }} />
              </div>
            </div>
            <div className="grid grid-cols-3 gap-px overflow-hidden rounded-xl border bg-border">
              <div className="bg-background px-3 py-3">
                <p className="text-[11px] text-muted-foreground">Admins</p>
                <p className="mt-1 text-lg font-semibold tabular-nums">{overview.admin_users}</p>
              </div>
              <div className="bg-background px-3 py-3">
                <p className="text-[11px] text-muted-foreground">Challenged</p>
                <p className="mt-1 text-lg font-semibold tabular-nums">{overview.guild_challenged}</p>
              </div>
              <div className="bg-background px-3 py-3">
                <p className="text-[11px] text-muted-foreground">Frozen</p>
                <p className="mt-1 text-lg font-semibold tabular-nums">{overview.guild_frozen}</p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-start gap-3 space-y-0">
            <span className="flex size-9 items-center justify-center rounded-xl bg-muted text-muted-foreground">
              <Bot aria-hidden="true" className="size-4" />
            </span>
            <div>
              <CardTitle>Next actions</CardTitle>
              <CardDescription className="mt-1">Jump straight into the areas operators use most.</CardDescription>
            </div>
          </CardHeader>
          <CardContent className="grid gap-2 px-5 pb-5 sm:grid-cols-2 sm:px-6">
            <Shortcut href="/users" icon={Users} label="Manage users" />
            <Shortcut href="/moderation" icon={ShieldAlert} label="Moderation queue" />
            <Shortcut href="/guild" icon={ShieldCheck} label="Guild review" />
            <Shortcut href="/agent-access" icon={Bot} label="Agent access" />
          </CardContent>
        </Card>
      </section>
    </div>
  );
}

function Shortcut({
  href,
  icon: Icon,
  label,
}: {
  href: string;
  icon: LucideIcon;
  label: string;
}) {
  return (
    <Link
      className="group flex items-center gap-3 rounded-xl border px-3 py-3 transition-colors hover:border-foreground/25 hover:bg-muted/50"
      href={href}
    >
      <span className="flex size-8 items-center justify-center rounded-lg bg-muted text-muted-foreground transition-colors group-hover:bg-foreground group-hover:text-background">
        <Icon aria-hidden="true" className="size-4" />
      </span>
      <span className="min-w-0 flex-1 text-sm font-medium">{label}</span>
      <ArrowUpRight aria-hidden="true" className="size-3.5 shrink-0 text-muted-foreground" />
    </Link>
  );
}

function pendingApprovalOrReview(overview: Overview) {
  return overview.pending_approval + overview.guild_challenged + overview.guild_frozen > 0;
}
