import { apiGetJson } from "@/lib/server-api";
import { requireAdminSession } from "@/lib/session";

import { ConfigForm } from "./ui/config-form";

type AdminConfig = {
  max_users_override: number | null;
  effective_max_users: number | null;
  notification_webhook_url: string | null;
};

export default async function ConfigPage() {
  await requireAdminSession();
  const config = await apiGetJson<AdminConfig>("/api/admin/config");

  return (
    <div className="space-y-4">
      <div>
        <h1 className="text-xl font-semibold">Config</h1>
        <p className="text-sm text-muted-foreground">
          Manage max users and notification webhook settings.
        </p>
      </div>

      <div className="rounded-lg border bg-background p-4 text-sm text-muted-foreground">
        Effective max users: {config.effective_max_users ?? "unlimited"}
      </div>

      <ConfigForm
        maxUsersOverride={config.max_users_override}
        notificationWebhookUrl={config.notification_webhook_url}
      />
    </div>
  );
}
