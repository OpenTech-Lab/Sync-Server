import { Badge } from "@/components/ui/badge";
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

function statusVariant(status: StickerItem["status"]) {
  if (status === "active") {
    return "default";
  }
  if (status === "rejected") {
    return "destructive";
  }
  return "secondary";
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function StickersTable({ stickers }: { stickers: StickerItem[] }) {
  return (
    <div className="overflow-hidden rounded-lg border">
      <Table>
        <TableHeader className="bg-muted/30">
          <TableRow>
            <TableHead>Name</TableHead>
            <TableHead>Group</TableHead>
            <TableHead>Type</TableHead>
            <TableHead>Size</TableHead>
            <TableHead>Status</TableHead>
            <TableHead>Uploader</TableHead>
            <TableHead>Created</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {stickers.length === 0 ? (
            <TableRow>
              <TableCell colSpan={7} className="py-10 text-center text-sm text-muted-foreground">
                No stickers found.
              </TableCell>
            </TableRow>
          ) : null}
          {stickers.map((sticker) => (
            <TableRow key={sticker.id}>
              <TableCell className="font-medium">{sticker.name}</TableCell>
              <TableCell className="text-sm text-muted-foreground">{sticker.group_name}</TableCell>
              <TableCell className="text-xs text-muted-foreground">{sticker.mime_type}</TableCell>
              <TableCell className="text-sm text-muted-foreground">{formatBytes(sticker.size_bytes)}</TableCell>
              <TableCell>
                <Badge className="text-xs" variant={statusVariant(sticker.status)}>{sticker.status}</Badge>
              </TableCell>
              <TableCell className="max-w-[140px] truncate text-xs text-muted-foreground">{sticker.uploader_id}</TableCell>
              <TableCell className="whitespace-nowrap text-sm text-muted-foreground">
                {new Date(sticker.created_at).toLocaleString()}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}
