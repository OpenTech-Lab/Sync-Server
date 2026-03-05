import Image from "next/image";
import Link from "next/link";

import { apiGetJson } from "@/lib/server-api";
import { requireAdminSession } from "@/lib/session";

import { CreateGroupForm } from "./ui/create-group-form";
import { GroupsList } from "./ui/groups-list";
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

export default async function StickerManagePage({
  searchParams,
}: {
  searchParams: Promise<{ group?: string }>;
}) {
  await requireAdminSession();
  const { group } = await searchParams;
  const stickers = await apiGetJson<StickerItem[]>("/api/stickers/list");

  // ── Group detail view ──────────────────────────────────────────────────────
  if (group) {
    const groupStickers = stickers.filter(
      (s) => s.group_name === group && s.name !== "__tab__",
    );
    const tabSticker = stickers.find(
      (s) => s.group_name === group && s.name === "__tab__",
    );

    return (
      <div className="space-y-8">
        <div className="flex items-center gap-3">
          <Link
            className="text-sm text-muted-foreground hover:text-foreground"
            href="/stickers/manage"
          >
            ← Groups
          </Link>
          <span className="text-muted-foreground">/</span>
          <div className="flex items-center gap-2">
            {tabSticker ? (
              <Image
                alt={group}
                className="h-7 w-7 rounded object-contain"
                height={28}
                src={`/api/admin/stickers/${tabSticker.id}/image`}
                unoptimized
                width={28}
              />
            ) : null}
            <h1 className="text-2xl font-semibold">{group}</h1>
          </div>
        </div>

        <StickerUploadForm groupName={group} />
        <StickerModeration stickers={groupStickers} />
      </div>
    );
  }

  // ── Groups list view ───────────────────────────────────────────────────────
  const pendingAll = stickers.filter(
    (s) => s.status === "pending" && s.name !== "__tab__",
  );

  return (
    <div className="space-y-8">
      <div>
        <h1 className="text-2xl font-semibold">Sticker Groups</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Create a group with a tab image, then open it to add stickers.
        </p>
      </div>

      <CreateGroupForm />
      <GroupsList stickers={stickers} />
      {pendingAll.length > 0 ? <StickerModeration stickers={pendingAll} /> : null}
    </div>
  );
}
