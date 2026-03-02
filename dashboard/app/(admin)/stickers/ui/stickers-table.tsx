type StickerItem = {
  id: string;
  uploader_id: string;
  name: string;
  mime_type: string;
  size_bytes: number;
  status: "active" | "pending" | "rejected";
  created_at: string;
};

export function StickersTable({ stickers }: { stickers: StickerItem[] }) {
  return (
    <div className="overflow-x-auto rounded-lg border bg-background">
      <table className="min-w-full text-sm">
        <thead className="border-b bg-muted/40 text-left">
          <tr>
            <th className="px-3 py-2 font-medium">Name</th>
            <th className="px-3 py-2 font-medium">MIME</th>
            <th className="px-3 py-2 font-medium">Size</th>
            <th className="px-3 py-2 font-medium">Status</th>
            <th className="px-3 py-2 font-medium">Uploader</th>
            <th className="px-3 py-2 font-medium">Created</th>
          </tr>
        </thead>
        <tbody>
          {stickers.map((sticker) => (
            <tr className="border-b" key={sticker.id}>
              <td className="px-3 py-2 font-medium">{sticker.name}</td>
              <td className="px-3 py-2">{sticker.mime_type}</td>
              <td className="px-3 py-2">{sticker.size_bytes}</td>
              <td className="px-3 py-2">{sticker.status}</td>
              <td className="px-3 py-2 text-muted-foreground">{sticker.uploader_id}</td>
              <td className="px-3 py-2 text-muted-foreground">
                {new Date(sticker.created_at).toLocaleString()}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
