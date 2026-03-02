import { apiGetJson } from "@/lib/server-api";
import { requireAdminSession } from "@/lib/session";

import { StickersTable } from "./ui/stickers-table";

type StickerItem = {
  id: string;
  uploader_id: string;
  name: string;
  mime_type: string;
  size_bytes: number;
  status: "active" | "pending" | "rejected";
  created_at: string;
};

export default async function StickersPage() {
  await requireAdminSession();
  const stickers = await apiGetJson<StickerItem[]>("/api/stickers/list");

  return (
    <div className="space-y-4">
      <div>
        <h1 className="text-xl font-semibold">Stickers</h1>
        <p className="text-sm text-muted-foreground">
          Review all sticker assets and current moderation state.
        </p>
      </div>
      <StickersTable stickers={stickers} />
    </div>
  );
}
