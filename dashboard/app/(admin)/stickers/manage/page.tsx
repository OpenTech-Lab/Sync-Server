import {
  Card,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
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
      <Card className="py-0">
        <CardHeader>
          <CardTitle className="text-2xl">Sticker Upload & Moderation</CardTitle>
          <CardDescription>
            Upload a sticker and approve/reject pending assets.
          </CardDescription>
        </CardHeader>
      </Card>

      <StickerUploadForm />
      <StickerModeration stickers={stickers} />
    </div>
  );
}
