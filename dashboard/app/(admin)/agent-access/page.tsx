import { apiGetJson } from "@/lib/server-api";
import { requireAdminSession } from "@/lib/session";

import { AgentAccessPanel, type AgentTokenView } from "./ui/agent-access-panel";

export default async function AgentAccessPage() {
  await requireAdminSession();
  const tokens = await apiGetJson<AgentTokenView[]>("/api/admin/agent-tokens");

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold">Agent Access</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Issue a token so an AI agent can connect to this server and manage content
          (planet settings, news, stickers) on your behalf. Agent tokens have full
          admin access — treat them like a password.
        </p>
      </div>
      <AgentAccessPanel initialTokens={tokens} />
    </div>
  );
}
