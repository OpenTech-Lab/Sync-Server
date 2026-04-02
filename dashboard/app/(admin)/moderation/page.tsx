import { Badge } from "@/components/ui/badge";
import { apiGetJson } from "@/lib/server-api";
import { requireAdminSession } from "@/lib/session";

import { ModerationReportsTable } from "./ui/moderation-reports-table";

export type ModerationReportItem = {
  id: string;
  reporter_user_id: string;
  reporter_username: string;
  reported_user_id: string;
  reported_username: string;
  source: string;
  content_kind: string;
  content_id: string | null;
  reason_code: string;
  reporter_note: string | null;
  content_excerpt: string | null;
  status: string;
  resolution_action: string | null;
  resolution_notes: string | null;
  review_due_at: string;
  reviewed_by_user_id: string | null;
  reviewed_at: string | null;
  created_at: string;
  updated_at: string;
};

const statusTabs = [
  { href: "/moderation", label: "Open", status: "open" },
  { href: "/moderation?status=resolved", label: "Resolved", status: "resolved" },
  { href: "/moderation?status=dismissed", label: "Dismissed", status: "dismissed" },
  { href: "/moderation?status=all", label: "All", status: "all" },
] as const;

export default async function ModerationPage({
  searchParams,
}: {
  searchParams: Promise<{ status?: string }>;
}) {
  await requireAdminSession();
  const { status } = await searchParams;
  const selectedStatus = (status ?? "open").trim().toLowerCase() || "open";
  const query = new URLSearchParams({
    status: selectedStatus,
    limit: "100",
  });
  const reports = await apiGetJson<ModerationReportItem[]>(
    `/api/admin/moderation/reports?${query.toString()}`,
  );

  return (
    <div className="space-y-6">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold">Moderation queue</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Review user reports and block-triggered moderation events.
          </p>
        </div>
        <Badge variant="outline" className="mt-1 shrink-0">
          {reports.length} reports
        </Badge>
      </div>

      <div className="flex items-center gap-3">
        {statusTabs.map((tab) => {
          const active = selectedStatus === tab.status;
          return (
            <a
              key={tab.href}
              href={tab.href}
              className={`pb-1 text-sm font-medium transition-colors border-b-2 ${
                active
                  ? "border-foreground"
                  : "border-transparent text-muted-foreground hover:text-foreground"
              }`}
            >
              {tab.label}
            </a>
          );
        })}
      </div>

      <ModerationReportsTable reports={reports} />
    </div>
  );
}
