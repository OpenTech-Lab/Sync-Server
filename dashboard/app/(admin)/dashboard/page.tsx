import { Badge } from "@/components/ui/badge";
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

function Stat({ label, value, note }: { label: string; value: string | number; note?: string }) {
  return (
    <div className="bg-background px-5 py-4">
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className="mt-1 text-2xl font-semibold tabular-nums">{value}</p>
      {note ? <p className="mt-0.5 text-[11px] text-muted-foreground/70">{note}</p> : null}
    </div>
  );
}

export default async function DashboardPage() {
  await requireAdminSession();
  const overview = await apiGetJson<Overview>("/api/admin/overview");
  const statusOk = overview.system_status.toLowerCase() === "ok";

  return (
    <div className="space-y-8">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold">Overview</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Instance status, user summary, and federation queue health.
          </p>
        </div>
        <Badge variant={statusOk ? "default" : "destructive"} className="shrink-0 mt-1">
          System: {overview.system_status}
        </Badge>
      </div>

      <section>
        <p className="mb-3 text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
          Users
        </p>
        <dl className="grid grid-cols-2 gap-px overflow-hidden rounded-lg border bg-border sm:grid-cols-3 lg:grid-cols-6">
          <Stat label="Total" value={overview.total_users} />
          <Stat label="Active" value={overview.active_users} />
          <Stat label="Admins" value={overview.admin_users} />
          <Stat label="Pending" value={overview.pending_approval} note="Awaiting approval" />
          <Stat label="Challenged" value={overview.guild_challenged} note="Guild review required" />
          <Stat label="Frozen" value={overview.guild_frozen} note="Progression frozen" />
        </dl>
      </section>

      <section>
        <p className="mb-3 text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
          Federation queue
        </p>
        <dl className="grid grid-cols-2 gap-px overflow-hidden rounded-lg border bg-border sm:grid-cols-3">
          <Stat label="Pending" value={overview.federation_pending} note="Queued outbound messages" />
          <Stat label="Failed" value={overview.federation_failed} />
          <Stat label="Dead-letter" value={overview.federation_dead_letter} />
        </dl>
      </section>
    </div>
  );
}
