import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
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
    <div className="space-y-4">
      <Card className="py-0">
        <CardHeader>
          <CardTitle className="text-2xl">Audit logs</CardTitle>
          <CardDescription>
            Recent admin actions and configuration changes.
          </CardDescription>
          <div>
            <Badge variant="outline">Entries: {logs.length}</Badge>
          </div>
        </CardHeader>
      </Card>

      <Card className="overflow-hidden py-0">
        <Table>
          <TableHeader className="bg-muted/40">
            <TableRow>
              <TableHead>When</TableHead>
              <TableHead>Action</TableHead>
              <TableHead>Target</TableHead>
              <TableHead>Details</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {logs.map((log) => (
              <TableRow key={log.id}>
                <TableCell className="text-muted-foreground">
                  {new Date(log.created_at).toLocaleString()}
                </TableCell>
                <TableCell className="font-medium">{log.action}</TableCell>
                <TableCell>{log.target ?? "-"}</TableCell>
                <TableCell className="align-top text-muted-foreground">
                  <pre className="max-w-xl overflow-auto whitespace-pre-wrap break-words text-xs">
                    {JSON.stringify(log.details, null, 2)}
                  </pre>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </Card>
    </div>
  );
}
