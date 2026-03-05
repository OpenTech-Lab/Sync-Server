"use client";

import { useRouter } from "next/navigation";
import { useRef, useState } from "react";

import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select";

export function CreateGroupForm() {
  const router = useRouter();
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const [groupName, setGroupName] = useState("");
  const [mimeType, setMimeType] = useState("image/png");
  const [file, setFile] = useState<File | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function onFileChange(e: React.ChangeEvent<HTMLInputElement>) {
    const picked = e.target.files?.[0] ?? null;
    setFile(picked);
    if (picked) {
      const url = URL.createObjectURL(picked);
      setPreview(url);
    } else {
      setPreview(null);
    }
  }

  async function onSubmit(event: React.FormEvent) {
    event.preventDefault();
    if (!groupName.trim()) {
      setError("Enter a group name.");
      return;
    }
    if (!file) {
      setError("Choose a tab image for the group.");
      return;
    }

    setError(null);
    setWorking(true);

    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      let binary = "";
      bytes.forEach((b) => {
        binary += String.fromCharCode(b);
      });
      const contentBase64 = btoa(binary);

      const response = await fetch("/api/admin/stickers/upload", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          group_name: groupName.trim(),
          name: "__tab__",
          mime_type: mimeType,
          content_base64: contentBase64,
        }),
      });

      if (!response.ok) {
        const body = await response.json().catch(() => ({}));
        setError((body as { error?: string }).error ?? "Failed to create group.");
      } else {
        router.push(`/stickers?group=${encodeURIComponent(groupName.trim())}`);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to create group.");
    } finally {
      setWorking(false);
    }
  }

  return (
    <section className="space-y-4">
      <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
        Create group
      </p>
      <form className="space-y-4" onSubmit={onSubmit}>
        <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
          <div className="space-y-2">
            <Label htmlFor="new-group-name">Group name</Label>
            <Input
              id="new-group-name"
              onChange={(e) => setGroupName(e.target.value)}
              placeholder="e.g. Happy"
              type="text"
              value={groupName}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="new-group-mime">Tab image type</Label>
            <NativeSelect
              id="new-group-mime"
              onChange={(e) => setMimeType(e.target.value)}
              value={mimeType}
            >
              <NativeSelectOption value="image/png">image/png</NativeSelectOption>
              <NativeSelectOption value="image/webp">image/webp</NativeSelectOption>
              <NativeSelectOption value="image/gif">image/gif</NativeSelectOption>
              <NativeSelectOption value="image/jpeg">image/jpeg</NativeSelectOption>
            </NativeSelect>
          </div>
          <div className="space-y-2">
            <Label htmlFor="new-group-tab-file">Tab image</Label>
            <Input
              accept="image/png,image/webp,image/gif,image/jpeg"
              className="hidden"
              id="new-group-tab-file"
              onChange={onFileChange}
              ref={fileInputRef}
              type="file"
            />
            <div className="flex items-center gap-3">
              {preview ? (
                // eslint-disable-next-line @next/next/no-img-element
                <img
                  alt="Tab preview"
                  className="h-10 w-10 rounded object-cover ring-1 ring-border"
                  src={preview}
                />
              ) : (
                <div className="flex h-10 w-10 items-center justify-center rounded bg-muted ring-1 ring-border">
                  <span className="text-xs text-muted-foreground">?</span>
                </div>
              )}
              <Button
                onClick={() => fileInputRef.current?.click()}
                size="sm"
                type="button"
                variant="outline"
              >
                Browse
              </Button>
              <span className="truncate text-sm text-muted-foreground">
                {file ? file.name : "No file selected"}
              </span>
            </div>
          </div>
        </div>

        {error ? (
          <Alert variant="destructive">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        ) : null}

        <Button disabled={working} size="sm" type="submit">
          {working ? "Creating…" : "Create group"}
        </Button>
      </form>
    </section>
  );
}
