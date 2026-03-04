import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
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
};

export default async function ConfigPage() {
  await requireAdminSession();
  const config = await apiGetJson<AdminConfig>("/api/admin/config");

  return (
    <div className="space-y-4">
      <Card className="py-0">
        <CardHeader>
          <CardTitle className="text-2xl">Config</CardTitle>
          <CardDescription>
            Manage planet profile and instance configuration.
          </CardDescription>
          <div>
            <Badge variant="outline">
              Effective max users: {config.effective_max_users ?? "unlimited"}
            </Badge>
          </div>
        </CardHeader>
      </Card>

      <ConfigForm
        maxUsersOverride={config.max_users_override}
        notificationWebhookUrl={config.notification_webhook_url}
        planetName={config.planet_name}
        planetDescription={config.planet_description}
        planetImageBase64={config.planet_image_base64}
        linkedPlanets={config.linked_planets}
      />
    </div>
  );
}
