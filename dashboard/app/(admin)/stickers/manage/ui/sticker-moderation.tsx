"use client";

import { useRouter } from "next/navigation";
import { useMemo, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
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
    <Card className="py-0">
      <CardHeader>
        <CardTitle className="text-lg">Moderation queue ({pendingCount} pending)</CardTitle>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Sticker</TableHead>
              <TableHead>Status</TableHead>
              <TableHead>Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {stickers.map((sticker) => (
              <TableRow key={sticker.id}>
                <TableCell>
                  <p className="font-medium">{sticker.name}</p>
                  <p className="text-muted-foreground">
                    {sticker.group_name} · {sticker.mime_type}
                  </p>
                </TableCell>
                <TableCell>
                  <Badge
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
                <TableCell>
                  <div className="flex gap-2">
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
      </CardContent>
    </Card>
  );
}
