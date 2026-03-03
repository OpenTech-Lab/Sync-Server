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
      <div>
        <h1 className="text-xl font-semibold">Audit logs</h1>
        <p className="text-sm text-muted-foreground">
          Recent admin actions and configuration changes.
        </p>
      </div>

      <div className="overflow-x-auto rounded-lg border bg-background">
        <table className="min-w-full text-sm">
          <thead className="border-b bg-muted/40 text-left">
            <tr>
              <th className="px-3 py-2 font-medium">When</th>
              <th className="px-3 py-2 font-medium">Action</th>
              <th className="px-3 py-2 font-medium">Target</th>
              <th className="px-3 py-2 font-medium">Details</th>
            </tr>
          </thead>
          <tbody>
            {logs.map((log) => (
              <tr className="border-b" key={log.id}>
                <td className="px-3 py-2 text-muted-foreground">
                  {new Date(log.created_at).toLocaleString()}
                </td>
                <td className="px-3 py-2 font-medium">{log.action}</td>
                <td className="px-3 py-2">{log.target ?? "-"}</td>
                <td className="px-3 py-2 text-muted-foreground">
                  <pre className="max-w-xl whitespace-pre-wrap break-words text-xs">
                    {JSON.stringify(log.details, null, 2)}
                  </pre>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
