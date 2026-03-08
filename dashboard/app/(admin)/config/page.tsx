import { apiGetJson } from "@/lib/server-api";
import { requireAdminSession } from "@/lib/session";

import { ConfigForm } from "./ui/config-form";
import { TrustPolicyForm } from "./ui/trust-policy-form";

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

type TrustPolicyConfig = {
  enforcement: {
    enabled: boolean;
    outbound_messages_enabled: boolean;
    friend_adds_enabled: boolean;
    attachment_sends_enabled: boolean;
  };
  daily_counter_retention_days: number;
  score_event_retention_days: number;
  level_policies: Array<{
    level: number;
    min_active_days: number;
    max_active_days: number | null;
    daily_outbound_messages_limit: number | null;
    daily_friend_add_limit: number | null;
    daily_attachment_send_limit: number | null;
  }>;
  rank_policies: Array<{
    rank: string;
    min_score: number;
    max_score: number | null;
    daily_outbound_messages_limit_multiplier_percent: number | null;
    daily_friend_add_limit_multiplier_percent: number | null;
    daily_attachment_send_limit_multiplier_percent: number | null;
    overrides_level_limits: boolean;
  }>;
  community_upvote_daily_cap: number;
  safe_attachment_types: string[];
};

export default async function ConfigPage() {
  await requireAdminSession();
  const [config, trustPolicy] = await Promise.all([
    apiGetJson<AdminConfig>("/api/admin/config"),
    apiGetJson<TrustPolicyConfig>("/api/admin/trust-policy"),
  ]);

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
      <TrustPolicyForm policy={trustPolicy} />
    </div>
  );
}
