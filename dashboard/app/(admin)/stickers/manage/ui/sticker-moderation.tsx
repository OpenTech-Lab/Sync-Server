"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";

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

export function StickerModeration({ stickers }: { stickers: StickerItem[] }) {
  const router = useRouter();
  const [workingId, setWorkingId] = useState<string | null>(null);

  async function moderate(stickerId: string, action: "approve" | "reject") {
    setWorkingId(stickerId);
    await fetch(`/api/admin/stickers/${stickerId}/moderate`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ action }),
    });
    setWorkingId(null);
    router.refresh();
  }

  return (
    <div className="space-y-3 rounded-lg border bg-background p-4">
      <h2 className="font-medium">Moderation Queue</h2>
      <div className="overflow-x-auto">
        <table className="min-w-full text-sm">
          <thead className="border-b text-left">
            <tr>
              <th className="px-3 py-2">Sticker</th>
              <th className="px-3 py-2">Status</th>
              <th className="px-3 py-2">Actions</th>
            </tr>
          </thead>
          <tbody>
            {stickers.map((sticker) => (
              <tr className="border-b" key={sticker.id}>
                <td className="px-3 py-2">
                  <p className="font-medium">{sticker.name}</p>
                  <p className="text-muted-foreground">
                    {sticker.group_name} · {sticker.mime_type}
                  </p>
                </td>
                <td className="px-3 py-2">{sticker.status}</td>
                <td className="px-3 py-2">
                  <div className="flex gap-2">
                    <button
                      className="rounded-md border px-3 py-1"
                      disabled={workingId === sticker.id}
                      onClick={() => moderate(sticker.id, "approve")}
                      type="button"
                    >
                      Approve
                    </button>
                    <button
                      className="rounded-md border px-3 py-1"
                      disabled={workingId === sticker.id}
                      onClick={() => moderate(sticker.id, "reject")}
                      type="button"
                    >
                      Reject
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
