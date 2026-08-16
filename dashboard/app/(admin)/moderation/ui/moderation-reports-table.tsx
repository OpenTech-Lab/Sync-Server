"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";

import { apiUrl } from "@/lib/client-api";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

import type { ModerationReportItem } from "../page";

const actionLabels: Record<string, string> = {
  dismiss: "Dismiss",
  remove_content: "Remove content",
  remove_content_and_limit_new_direct_messages: "Yellow card",
  suspend_user: "Suspend user",
  remove_content_and_suspend_user: "Remove + suspend",
};

export function ModerationReportsTable({
  reports,
}: {
  reports: ModerationReportItem[];
}) {
  const router = useRouter();
  const [workingReportId, setWorkingReportId] = useState<string | null>(null);

  async function resolveReport(reportId: string, resolutionAction: string) {
    setWorkingReportId(reportId);
    await fetch(apiUrl(`/api/admin/moderation/reports/${reportId}/resolve`), {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ resolution_action: resolutionAction }),
    });
    setWorkingReportId(null);
    router.refresh();
  }

  return (
    <div className="overflow-hidden rounded-lg border">
      <Table>
        <TableHeader className="bg-muted/30">
          <TableRow>
            <TableHead>Reported user</TableHead>
            <TableHead>Reporter</TableHead>
            <TableHead>Reason</TableHead>
            <TableHead>Content</TableHead>
            <TableHead>Due</TableHead>
            <TableHead>Status</TableHead>
            <TableHead className="text-right">Actions</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {reports.length === 0 ? (
            <TableRow>
              <TableCell colSpan={7} className="py-10 text-center text-sm text-muted-foreground">
                No moderation reports found.
              </TableCell>
            </TableRow>
          ) : null}
          {reports.map((report) => {
            const isOpen = report.status === "open";
            const isWorking = workingReportId === report.id;
            return (
              <TableRow key={report.id}>
                <TableCell className="align-top">
                  <p className="font-medium">{report.reported_username}</p>
                  <p className="text-xs text-muted-foreground">{report.reported_user_id}</p>
                </TableCell>
                <TableCell className="align-top">
                  <p className="text-sm">{report.reporter_username}</p>
                  <p className="text-xs text-muted-foreground">{report.reporter_user_id}</p>
                </TableCell>
                <TableCell className="align-top">
                  <p className="text-sm font-medium">{report.reason_code}</p>
                  <p className="text-xs text-muted-foreground">
                    {report.source} · {report.content_kind}
                  </p>
                  {report.reporter_note ? (
                    <p className="mt-2 text-xs text-muted-foreground whitespace-pre-wrap">
                      {report.reporter_note}
                    </p>
                  ) : null}
                </TableCell>
                <TableCell className="align-top">
                  <pre className="max-w-sm whitespace-pre-wrap break-words text-xs text-muted-foreground">
                    {report.content_excerpt?.trim() || "—"}
                  </pre>
                </TableCell>
                <TableCell className="align-top text-sm text-muted-foreground whitespace-nowrap">
                  {new Date(report.review_due_at).toLocaleString()}
                </TableCell>
                <TableCell className="align-top">
                  <div className="space-y-2">
                    <Badge variant={isOpen ? "default" : "outline"}>{report.status}</Badge>
                    {report.resolution_action ? (
                      <p className="text-xs text-muted-foreground">
                        {actionLabels[report.resolution_action] ?? report.resolution_action}
                      </p>
                    ) : null}
                  </div>
                </TableCell>
                <TableCell className="align-top">
                  {isOpen ? (
                    <div className="flex flex-wrap justify-end gap-2">
                      <Button
                        size="sm"
                        type="button"
                        variant="secondary"
                        disabled={isWorking}
                        onClick={() => resolveReport(report.id, "dismiss")}
                      >
                        {isWorking ? "Working…" : "Dismiss"}
                      </Button>
                      <Button
                        size="sm"
                        type="button"
                        variant="outline"
                        disabled={isWorking}
                        onClick={() => resolveReport(report.id, "remove_content")}
                      >
                        Remove
                      </Button>
                      <Button
                        size="sm"
                        type="button"
                        variant="outline"
                        disabled={isWorking}
                        onClick={() =>
                          resolveReport(
                            report.id,
                            "remove_content_and_limit_new_direct_messages",
                          )
                        }
                      >
                        Yellow card
                      </Button>
                      <Button
                        size="sm"
                        type="button"
                        variant="outline"
                        disabled={isWorking}
                        onClick={() => resolveReport(report.id, "suspend_user")}
                      >
                        Suspend
                      </Button>
                      <Button
                        size="sm"
                        type="button"
                        variant="destructive"
                        disabled={isWorking}
                        onClick={() =>
                          resolveReport(report.id, "remove_content_and_suspend_user")
                        }
                      >
                        Remove + suspend
                      </Button>
                    </div>
                  ) : (
                    <p className="text-right text-xs text-muted-foreground">
                      Reviewed{" "}
                      {report.reviewed_at
                        ? new Date(report.reviewed_at).toLocaleString()
                        : "—"}
                    </p>
                  )}
                </TableCell>
              </TableRow>
            );
          })}
        </TableBody>
      </Table>
    </div>
  );
}
