"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";

export function ConfigForm({
  maxUsersOverride,
  notificationWebhookUrl,
}: {
  maxUsersOverride: number | null;
  notificationWebhookUrl: string | null;
}) {
  const router = useRouter();
  const [maxUsers, setMaxUsers] = useState(
    maxUsersOverride ? String(maxUsersOverride) : "",
  );
  const [webhookUrl, setWebhookUrl] = useState(notificationWebhookUrl ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function onSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSaving(true);
    setError(null);

    const payload = {
      max_users: maxUsers.trim() ? Number(maxUsers.trim()) : null,
      notification_webhook_url: webhookUrl.trim() || null,
    };

    const response = await fetch("/api/admin/config", {
      method: "PUT",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(payload),
    });

    if (!response.ok) {
      const body = (await response.json().catch(() => null)) as
        | { error?: string }
        | null;
      setError(body?.error ?? "Failed to update config");
      setSaving(false);
      return;
    }

    router.refresh();
    setSaving(false);
  }

  return (
    <form className="space-y-4 rounded-lg border bg-background p-4" onSubmit={onSubmit}>
      <label className="block text-sm">
        <span className="mb-1 block text-muted-foreground">Max users override</span>
        <input
          className="w-full rounded-md border px-3 py-2"
          min={1}
          onChange={(event) => setMaxUsers(event.target.value)}
          placeholder="Leave blank for env/default"
          type="number"
          value={maxUsers}
        />
      </label>

      <label className="block text-sm">
        <span className="mb-1 block text-muted-foreground">Notification webhook URL</span>
        <input
          className="w-full rounded-md border px-3 py-2"
          onChange={(event) => setWebhookUrl(event.target.value)}
          placeholder="https://..."
          type="url"
          value={webhookUrl}
        />
      </label>

      {error ? <p className="text-sm text-destructive">{error}</p> : null}

      <button
        className="rounded-md bg-primary px-4 py-2 text-primary-foreground disabled:opacity-70"
        disabled={saving}
        type="submit"
      >
        {saving ? "Saving..." : "Save settings"}
      </button>
    </form>
  );
}
