import {
  Card,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { apiGetJson } from "@/lib/server-api";
import { requireAdminSession } from "@/lib/session";

import { StickersTable } from "./ui/stickers-table";

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

export default async function StickersPage() {
  await requireAdminSession();
  const stickers = await apiGetJson<StickerItem[]>("/api/stickers/list");

  return (
    <div className="space-y-4">
      <Card className="py-0">
        <CardHeader>
          <CardTitle className="text-2xl">Stickers</CardTitle>
          <CardDescription>
            Review all sticker assets and current moderation state.
          </CardDescription>
        </CardHeader>
      </Card>
      <StickersTable stickers={stickers} />
    </div>
  );
}
