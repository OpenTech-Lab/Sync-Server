"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";

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
    <form className="space-y-3 rounded-lg border bg-background p-4" onSubmit={onSubmit}>
      <h2 className="font-medium">Upload Sticker</h2>
      <div className="grid gap-3 md:grid-cols-4">
        <input
          className="rounded-md border bg-background px-3 py-2"
          onChange={(e) => setGroupName(e.target.value)}
          placeholder="Group"
          type="text"
          value={groupName}
        />
        <input
          className="rounded-md border bg-background px-3 py-2"
          onChange={(e) => setName(e.target.value)}
          placeholder="Sticker name"
          type="text"
          value={name}
        />
        <select
          className="rounded-md border bg-background px-3 py-2"
          onChange={(e) => setMimeType(e.target.value)}
          value={mimeType}
        >
          <option value="image/png">image/png</option>
          <option value="image/webp">image/webp</option>
          <option value="image/gif">image/gif</option>
          <option value="image/jpeg">image/jpeg</option>
        </select>
        <input
          accept="image/png,image/webp,image/gif,image/jpeg"
          className="rounded-md border bg-background px-3 py-2"
          onChange={(e) => setFile(e.target.files?.[0] ?? null)}
          type="file"
        />
      </div>
      {error && <p className="text-sm text-red-600">{error}</p>}
      <button className="rounded-md border px-4 py-2" disabled={working} type="submit">
        {working ? "Uploading..." : "Upload"}
      </button>
    </form>
  );
}
