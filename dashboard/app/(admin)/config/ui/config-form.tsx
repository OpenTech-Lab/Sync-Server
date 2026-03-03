"use client";

import Image from "next/image";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";

export function ConfigForm({
  maxUsersOverride,
  notificationWebhookUrl,
  planetName,
  planetDescription,
  planetImageBase64,
}: {
  maxUsersOverride: number | null;
  notificationWebhookUrl: string | null;
  planetName: string | null;
  planetDescription: string | null;
  planetImageBase64: string | null;
}) {
  const router = useRouter();
  const imageInputRef = useRef<HTMLInputElement | null>(null);
  const [maxUsers, setMaxUsers] = useState(
    maxUsersOverride ? String(maxUsersOverride) : "",
  );
  const [webhookUrl, setWebhookUrl] = useState(notificationWebhookUrl ?? "");
  const [nextPlanetName, setNextPlanetName] = useState(planetName ?? "");
  const [nextPlanetDescription, setNextPlanetDescription] = useState(
    planetDescription ?? "",
  );
  const [nextPlanetImageBase64, setNextPlanetImageBase64] = useState(
    planetImageBase64 ?? "",
  );
  const [uploadError, setUploadError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function onSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSaving(true);
    setError(null);

    const payload = {
      max_users: maxUsers.trim() ? Number(maxUsers.trim()) : null,
      notification_webhook_url: webhookUrl.trim() || null,
      planet_name: nextPlanetName.trim() || null,
      planet_description: nextPlanetDescription.trim() || null,
      planet_image_base64: nextPlanetImageBase64.trim() || null,
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

  function onImageSelected(event: React.ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (!file) {
      return;
    }

    setUploadError(null);
    const allowedTypes = new Set(["image/png", "image/jpeg", "image/webp"]);
    if (!allowedTypes.has(file.type)) {
      setUploadError("Use PNG, JPEG, or WebP.");
      event.target.value = "";
      return;
    }

    if (file.size > 20 * 1024 * 1024) {
      setUploadError("Image must be 20MB or smaller.");
      event.target.value = "";
      return;
    }

    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result !== "string") {
        setUploadError("Unable to read image.");
        return;
      }
      setNextPlanetImageBase64(reader.result);
      event.target.value = "";
    };
    reader.onerror = () => {
      setUploadError("Unable to read image.");
      event.target.value = "";
    };
    reader.readAsDataURL(file);
  }

  function clearImage() {
    setUploadError(null);
    setNextPlanetImageBase64("");
    if (imageInputRef.current) {
      imageInputRef.current.value = "";
    }
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
        <span className="mb-1 block text-muted-foreground">Planet name</span>
        <input
          className="w-full rounded-md border px-3 py-2"
          onChange={(event) => setNextPlanetName(event.target.value)}
          placeholder="My Planet"
          type="text"
          value={nextPlanetName}
        />
      </label>

      <label className="block text-sm">
        <span className="mb-1 block text-muted-foreground">Planet description</span>
        <textarea
          className="w-full rounded-md border px-3 py-2"
          onChange={(event) => setNextPlanetDescription(event.target.value)}
          placeholder="A short description shown on onboarding"
          rows={3}
          value={nextPlanetDescription}
        />
      </label>

      <div className="space-y-2 text-sm">
        <span className="block text-muted-foreground">Planet image</span>
        {nextPlanetImageBase64 ? (
          <div className="rounded-md border p-2">
            <Image
              alt="Planet preview"
              className="h-32 w-32 rounded-md object-cover"
              height={128}
              src={nextPlanetImageBase64}
              unoptimized
              width={128}
            />
          </div>
        ) : null}
        <div className="flex flex-wrap items-center gap-2">
          <input
            accept="image/png,image/jpeg,image/webp"
            className="block w-full max-w-sm rounded-md border px-3 py-2 text-sm"
            onChange={onImageSelected}
            ref={imageInputRef}
            type="file"
          />
          {nextPlanetImageBase64 ? (
            <button
              className="rounded-md border px-3 py-2 text-xs"
              onClick={clearImage}
              type="button"
            >
              Remove image
            </button>
          ) : null}
        </div>
        <p className="text-xs text-muted-foreground">
          PNG/JPEG/WebP, up to 20MB (compressed automatically after upload).
        </p>
        {uploadError ? <p className="text-sm text-destructive">{uploadError}</p> : null}
      </div>

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
