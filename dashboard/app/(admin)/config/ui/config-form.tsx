"use client";

import Image from "next/image";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";

import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { Textarea } from "@/components/ui/textarea";

export function ConfigForm({
  maxUsersOverride,
  notificationWebhookUrl,
  planetName,
  planetDescription,
  planetImageBase64,
  linkedPlanets,
}: {
  maxUsersOverride: number | null;
  notificationWebhookUrl: string | null;
  planetName: string | null;
  planetDescription: string | null;
  planetImageBase64: string | null;
  linkedPlanets: string[];
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
  const [nextLinkedPlanets, setNextLinkedPlanets] = useState(
    linkedPlanets.join("\n"),
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
      linked_planets: nextLinkedPlanets
        .split("\n")
        .map((item) => item.trim())
        .filter((item) => item.length > 0),
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
    <form className="space-y-8" onSubmit={onSubmit}>
      <section className="space-y-4">
        <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">Instance</p>
        <div className="grid gap-4 sm:grid-cols-2">
          <div className="space-y-2">
            <Label htmlFor="max-users-override">Max users override</Label>
            <Input
              id="max-users-override"
              min={1}
              onChange={(event) => setMaxUsers(event.target.value)}
              placeholder="Leave blank for env/default"
              type="number"
              value={maxUsers}
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="webhook-url">Notification webhook URL</Label>
            <Input
              id="webhook-url"
              onChange={(event) => setWebhookUrl(event.target.value)}
              placeholder="https://…"
              type="url"
              value={webhookUrl}
            />
          </div>
        </div>
      </section>

      <Separator />

      <section className="space-y-4">
        <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">Planet profile</p>

        <div className="space-y-2">
          <Label htmlFor="planet-name">Planet name</Label>
          <Input
            id="planet-name"
            onChange={(event) => setNextPlanetName(event.target.value)}
            placeholder="My Planet"
            type="text"
            value={nextPlanetName}
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor="planet-description">Planet description</Label>
          <Textarea
            id="planet-description"
            onChange={(event) => setNextPlanetDescription(event.target.value)}
            placeholder="A short description shown on onboarding"
            rows={3}
            value={nextPlanetDescription}
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor="planet-image">Planet image</Label>
          {nextPlanetImageBase64 ? (
            <div className="w-fit rounded-md border p-2">
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
          <Input
            accept="image/png,image/jpeg,image/webp"
            id="planet-image"
            onChange={onImageSelected}
            ref={imageInputRef}
            type="file"
          />
          <p className="text-xs text-muted-foreground">
            PNG/JPEG/WebP, up to 20 MB (compressed automatically after upload).
          </p>
          {nextPlanetImageBase64 ? (
            <Button onClick={clearImage} size="sm" type="button" variant="outline">
              Remove image
            </Button>
          ) : null}
          {uploadError ? (
            <Alert variant="destructive">
              <AlertDescription>{uploadError}</AlertDescription>
            </Alert>
          ) : null}
        </div>
      </section>

      <Separator />

      <section className="space-y-4">
        <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">Federation</p>
        <div className="space-y-2">
          <Label htmlFor="linked-planets">Connected planet servers</Label>
          <Textarea
            className="font-mono text-xs"
            id="linked-planets"
            onChange={(event) => setNextLinkedPlanets(event.target.value)}
            placeholder={"https://planet-a.example.com\nhttps://planet-b.example.com"}
            rows={4}
            value={nextLinkedPlanets}
          />
          <p className="text-xs text-muted-foreground">
            One URL per line. These planets will appear in mobile Home.
          </p>
        </div>
      </section>

      {error ? (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      ) : null}

      <Button disabled={saving} type="submit">
        {saving ? "Saving…" : "Save settings"}
      </Button>
    </form>
  );
}
