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
  federation_pending: number;
  federation_failed: number;
  federation_dead_letter: number;
};

function StatCard({
  title,
  value,
  caption,
}: {
  title: string;
  value: string | number;
  caption?: string;
}) {
  return (
    <Card className="py-0">
      <CardHeader className="pb-2">
        <CardDescription>{title}</CardDescription>
        <CardTitle className="text-2xl">{value}</CardTitle>
      </CardHeader>
      {caption ? (
        <CardContent className="pb-4 text-xs text-muted-foreground">{caption}</CardContent>
      ) : null}
    </Card>
  );
}

export default async function DashboardPage() {
  await requireAdminSession();
  const overview = await apiGetJson<Overview>("/api/admin/overview");
  const statusVariant =
    overview.system_status.toLowerCase() === "ok" ? "default" : "destructive";

  return (
    <div className="space-y-6">
      <Card className="py-0">
        <CardHeader>
          <CardTitle className="text-2xl">Overview</CardTitle>
          <CardDescription>
            Instance status, user summary, and federation queue health.
          </CardDescription>
          <div>
            <Badge variant={statusVariant}>System: {overview.system_status}</Badge>
          </div>
        </CardHeader>
      </Card>

      <section className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard title="Total users" value={overview.total_users} />
        <StatCard title="Active users" value={overview.active_users} />
        <StatCard title="Admin users" value={overview.admin_users} />
        <StatCard
          title="Federation pending"
          value={overview.federation_pending}
          caption="Queued outbound federation messages"
        />
      </section>

      <section className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
        <StatCard title="Federation failed" value={overview.federation_failed} />
        <StatCard
          title="Federation dead-letter"
          value={overview.federation_dead_letter}
        />
      </section>
    </div>
  );
}
