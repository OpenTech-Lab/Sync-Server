import { apiGetJson } from "@/lib/server-api";
import { requireAdminSession } from "@/lib/session";

import { ConfigForm } from "./ui/config-form";

type AdminConfig = {
  max_users_override: number | null;
  effective_max_users: number | null;
  notification_webhook_url: string | null;
  planet_name: string | null;
  planet_description: string | null;
  planet_image_base64: string | null;
  linked_planets: string[];
  require_approval: boolean;
};

export default async function ConfigPage() {
  await requireAdminSession();
  const config = await apiGetJson<AdminConfig>("/api/admin/config");

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold">Config</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Manage planet profile and instance configuration.
          {" "}
          <span className="text-muted-foreground/70">
            Effective max users: <strong className="font-medium text-foreground">{config.effective_max_users ?? "unlimited"}</strong>
          </span>
        </p>
      </div>

      <ConfigForm
        maxUsersOverride={config.max_users_override}
        notificationWebhookUrl={config.notification_webhook_url}
        planetName={config.planet_name}
        planetDescription={config.planet_description}
        planetImageBase64={config.planet_image_base64}
        linkedPlanets={config.linked_planets}
        requireApproval={config.require_approval}
      />
    </div>
  );
}
