import { apiGetJson } from "@/lib/server-api";
import { requireAdminSession } from "@/lib/session";

type Overview = {
  system_status: string;
  total_users: number;
  active_users: number;
  admin_users: number;
  federation_pending: number;
  federation_failed: number;
  federation_dead_letter: number;
};

function StatCard({
  title,
  value,
}: {
  title: string;
  value: string | number;
}) {
  return (
    <div className="rounded-lg border bg-background p-4">
      <p className="text-sm text-muted-foreground">{title}</p>
      <p className="mt-2 text-2xl font-semibold">{value}</p>
    </div>
  );
}

export default async function DashboardPage() {
  await requireAdminSession();
  const overview = await apiGetJson<Overview>("/api/admin/overview");

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-xl font-semibold">Overview</h1>
        <p className="text-sm text-muted-foreground">
          Instance status, user summary, and federation queue health.
        </p>
      </div>

      <section className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard title="System status" value={overview.system_status} />
        <StatCard title="Total users" value={overview.total_users} />
        <StatCard title="Active users" value={overview.active_users} />
        <StatCard title="Admin users" value={overview.admin_users} />
      </section>

      <section className="grid grid-cols-1 gap-4 sm:grid-cols-3">
        <StatCard title="Federation pending" value={overview.federation_pending} />
        <StatCard title="Federation failed" value={overview.federation_failed} />
        <StatCard
          title="Federation dead-letter"
          value={overview.federation_dead_letter}
        />
      </section>
    </div>
  );
}
