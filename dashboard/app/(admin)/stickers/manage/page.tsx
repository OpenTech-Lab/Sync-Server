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
    <div className="space-y-6">
      <div>
        <h1 className="text-xl font-semibold">Sticker Upload & Moderation</h1>
        <p className="text-sm text-muted-foreground">
          Upload a sticker and approve/reject pending assets.
        </p>
      </div>

      <StickerUploadForm />
      <StickerModeration stickers={stickers} />
    </div>
  );
}
