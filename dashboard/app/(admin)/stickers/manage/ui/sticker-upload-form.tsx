"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";

import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select";

export function StickerUploadForm() {
  const router = useRouter();
  const [groupName, setGroupName] = useState("General");
  const [name, setName] = useState("");
  const [mimeType, setMimeType] = useState("image/png");
  const [file, setFile] = useState<File | null>(null);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function onSubmit(event: React.FormEvent) {
    event.preventDefault();
    if (!file) {
      setError("Choose an image file first.");
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
          group_name: groupName,
          name: name || file.name,
          mime_type: mimeType,
          content_base64: contentBase64,
        }),
      });

      if (!response.ok) {
        setError("Upload failed.");
      } else {
        setName("");
        setGroupName("General");
        setFile(null);
        router.refresh();
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Upload failed.");
    } finally {
      setWorking(false);
    }
  }

  return (
    <Card className="py-0">
      <CardHeader>
        <CardTitle className="text-lg">Upload sticker</CardTitle>
      </CardHeader>
      <CardContent>
        <form className="space-y-4" onSubmit={onSubmit}>
          <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
            <div className="space-y-2">
              <Label htmlFor="sticker-group">Group</Label>
              <Input
                id="sticker-group"
                onChange={(e) => setGroupName(e.target.value)}
                placeholder="Group"
                type="text"
                value={groupName}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="sticker-name">Sticker name</Label>
              <Input
                id="sticker-name"
                onChange={(e) => setName(e.target.value)}
                placeholder="Sticker name"
                type="text"
                value={name}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="sticker-mime">MIME type</Label>
              <NativeSelect
                id="sticker-mime"
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
              <Label htmlFor="sticker-file">File</Label>
              <Input
                accept="image/png,image/webp,image/gif,image/jpeg"
                id="sticker-file"
                onChange={(e) => setFile(e.target.files?.[0] ?? null)}
                type="file"
              />
            </div>
          </div>

          {error ? (
            <Alert variant="destructive">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          ) : null}

          <Button disabled={working} type="submit">
            {working ? "Uploading..." : "Upload"}
          </Button>
        </form>
      </CardContent>
    </Card>
  );
}
