import { apiGetJson } from "@/lib/server-api";
import { requireAdminSession } from "@/lib/session";

import { StickerModeration } from "./ui/sticker-moderation";
import { StickerUploadForm } from "./ui/sticker-upload-form";

type StickerItem = {
  id: string;
  uploader_id: string;
  group_name: string;
  name: string;
  mime_type: string;
  size_bytes: number;
  status: "active" | "pending" | "rejected";
  created_at: string;
};

export default async function StickerManagePage() {
  await requireAdminSession();
  const stickers = await apiGetJson<StickerItem[]>("/api/stickers/list");

  return (
    <div className="space-y-8">
      <div>
        <h1 className="text-2xl font-semibold">Sticker Moderation</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Upload stickers and approve or reject pending assets.
        </p>
      </div>

      <StickerUploadForm />
      <StickerModeration stickers={stickers} />
    </div>
  );
}
