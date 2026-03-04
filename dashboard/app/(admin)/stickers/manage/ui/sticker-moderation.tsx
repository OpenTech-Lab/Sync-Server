"use client";

import { useRouter } from "next/navigation";
import { useMemo, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

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

  const pendingCount = useMemo(
    () => stickers.filter((item) => item.status === "pending").length,
    [stickers],
  );

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
    <section className="space-y-4">
      <Separator />
      <div className="flex items-center gap-2">
        <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">Moderation queue</p>
        {pendingCount > 0 ? (
          <Badge variant="secondary" className="text-xs">{pendingCount} pending</Badge>
        ) : null}
      </div>
      <div className="overflow-hidden rounded-lg border">
        <Table>
          <TableHeader className="bg-muted/30">
            <TableRow>
              <TableHead>Sticker</TableHead>
              <TableHead>Status</TableHead>
              <TableHead className="text-right">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {stickers.length === 0 ? (
              <TableRow>
                <TableCell colSpan={3} className="py-10 text-center text-sm text-muted-foreground">
                  No stickers to moderate.
                </TableCell>
              </TableRow>
            ) : null}
            {stickers.map((sticker) => (
              <TableRow key={sticker.id}>
                <TableCell>
                  <p className="font-medium">{sticker.name}</p>
                  <p className="text-xs text-muted-foreground">
                    {sticker.group_name} · {sticker.mime_type}
                  </p>
                </TableCell>
                <TableCell>
                  <Badge
                    className="text-xs"
                    variant={
                      sticker.status === "rejected"
                        ? "destructive"
                        : sticker.status === "active"
                          ? "default"
                          : "secondary"
                    }
                  >
                    {sticker.status}
                  </Badge>
                </TableCell>
                <TableCell className="text-right">
                  <div className="flex justify-end gap-2">
                    <Button
                      disabled={workingId === sticker.id}
                      onClick={() => moderate(sticker.id, "approve")}
                      size="sm"
                      type="button"
                      variant="secondary"
                    >
                      Approve
                    </Button>
                    <Button
                      disabled={workingId === sticker.id}
                      onClick={() => moderate(sticker.id, "reject")}
                      size="sm"
                      type="button"
                      variant="destructive"
                    >
                      Reject
                    </Button>
                  </div>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
    </section>
  );
}
