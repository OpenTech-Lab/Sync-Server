import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
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

export function StickersTable({ stickers }: { stickers: StickerItem[] }) {
  return (
    <Card className="overflow-hidden py-0">
      <Table>
        <TableHeader className="bg-muted/40">
          <TableRow>
            <TableHead>Name</TableHead>
            <TableHead>Group</TableHead>
            <TableHead>MIME</TableHead>
            <TableHead>Size</TableHead>
            <TableHead>Status</TableHead>
            <TableHead>Uploader</TableHead>
            <TableHead>Created</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {stickers.map((sticker) => (
            <TableRow key={sticker.id}>
              <TableCell className="font-medium">{sticker.name}</TableCell>
              <TableCell>{sticker.group_name}</TableCell>
              <TableCell>{sticker.mime_type}</TableCell>
              <TableCell>{sticker.size_bytes}</TableCell>
              <TableCell>
                <Badge variant={statusVariant(sticker.status)}>{sticker.status}</Badge>
              </TableCell>
              <TableCell className="text-muted-foreground">{sticker.uploader_id}</TableCell>
              <TableCell className="text-muted-foreground">
                {new Date(sticker.created_at).toLocaleString()}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </Card>
  );
}
