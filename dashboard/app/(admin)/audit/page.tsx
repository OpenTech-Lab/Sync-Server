import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { apiGetJson } from "@/lib/server-api";
import { requireAdminSession } from "@/lib/session";

type AuditLog = {
  id: string;
  actor_user_id: string | null;
  action: string;
  target: string | null;
  details: Record<string, unknown>;
  created_at: string;
};

export default async function AuditPage() {
  await requireAdminSession();
  const logs = await apiGetJson<AuditLog[]>("/api/admin/audit-logs?limit=100");

  return (
    <div className="space-y-6">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold">Audit logs</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Recent admin actions and configuration changes.
          </p>
        </div>
        <Badge variant="outline" className="mt-1 shrink-0">{logs.length} entries</Badge>
      </div>

      <div className="overflow-hidden rounded-lg border">
        <Table>
          <TableHeader className="bg-muted/30">
            <TableRow>
              <TableHead>When</TableHead>
              <TableHead>Action</TableHead>
              <TableHead>Target</TableHead>
              <TableHead>Details</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {logs.length === 0 ? (
              <TableRow>
                <TableCell colSpan={4} className="py-10 text-center text-sm text-muted-foreground">
                  No audit log entries found.
                </TableCell>
              </TableRow>
            ) : null}
            {logs.map((log) => (
              <TableRow key={log.id}>
                <TableCell className="text-sm text-muted-foreground whitespace-nowrap">
                  {new Date(log.created_at).toLocaleString()}
                </TableCell>
                <TableCell className="font-medium text-sm">{log.action}</TableCell>
                <TableCell className="text-sm">{log.target ?? "—"}</TableCell>
                <TableCell className="align-top">
                  <pre className="max-w-xl overflow-auto whitespace-pre-wrap break-words text-xs text-muted-foreground">
                    {JSON.stringify(log.details, null, 2)}
                  </pre>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
    </div>
  );
}
