import Image from "next/image";
import Link from "next/link";

import { Separator } from "@/components/ui/separator";

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

type Group = {
  name: string;
  tabStickerId: string | null;
  total: number;
  pending: number;
};

function deriveGroups(stickers: StickerItem[]): Group[] {
  const map = new Map<string, Group>();

  for (const s of stickers) {
    if (!map.has(s.group_name)) {
      map.set(s.group_name, {
        name: s.group_name,
        tabStickerId: null,
        total: 0,
        pending: 0,
      });
    }
    const group = map.get(s.group_name)!;
    if (s.name === "__tab__") {
      group.tabStickerId = s.id;
    } else {
      group.total += 1;
      if (s.status === "pending") {
        group.pending += 1;
      }
    }
  }

  return Array.from(map.values()).sort((a, b) => a.name.localeCompare(b.name));
}

export function GroupsList({ stickers }: { stickers: StickerItem[] }) {
  const groups = deriveGroups(stickers);

  return (
    <section className="space-y-4">
      <Separator />
      <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
        Groups ({groups.length})
      </p>
      {groups.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          No groups yet. Create one above.
        </p>
      ) : (
        <div className="overflow-hidden rounded-lg border">
          {groups.map((group, idx) => (
            <Link
              className="group flex items-center gap-4 px-4 py-3 transition-colors hover:bg-accent"
              href={`/stickers?group=${encodeURIComponent(group.name)}`}
              key={group.name}
              style={idx > 0 ? { borderTop: "1px solid hsl(var(--border))" } : undefined}
            >
              <div className="flex h-12 w-12 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-muted/40">
                {group.tabStickerId ? (
                  <Image
                    alt={group.name}
                    className="h-12 w-12 object-contain"
                    height={48}
                    src={`/api/admin/stickers/${group.tabStickerId}/image`}
                    unoptimized
                    width={48}
                  />
                ) : (
                  <span className="text-xl text-muted-foreground">🖼</span>
                )}
              </div>
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-medium">{group.name}</p>
                <p className="text-xs text-muted-foreground">
                  {group.total} sticker{group.total !== 1 ? "s" : ""}
                  {group.pending > 0 ? (
                    <span className="ml-1 text-amber-500">· {group.pending} pending</span>
                  ) : null}
                </p>
              </div>
              <span className="text-xs text-muted-foreground group-hover:text-foreground">→</span>
            </Link>
          ))}
        </div>
      )}
    </section>
  );
}
