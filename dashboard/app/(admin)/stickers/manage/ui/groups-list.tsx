"use client";

import Image from "next/image";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
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
  group_author?: string | null;
};

type Group = {
  name: string;
  tabStickerId: string | null;
  author: string | null;
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
        author: null,
        total: 0,
        pending: 0,
      });
    }
    const group = map.get(s.group_name)!;
    if (s.name === "__tab__") {
      group.tabStickerId = s.id;
      group.author = s.group_author ?? null;
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
  const router = useRouter();
  const groups = deriveGroups(stickers);

  // Rename state
  const [editing, setEditing] = useState<string | null>(null);
  const [newName, setNewName] = useState("");
  const [renaming, setRenaming] = useState(false);
  const [renameError, setRenameError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  // Author edit state
  const [editingAuthor, setEditingAuthor] = useState<string | null>(null);
  const [newAuthor, setNewAuthor] = useState("");
  const [savingAuthor, setSavingAuthor] = useState(false);
  const [authorError, setAuthorError] = useState<string | null>(null);
  const authorInputRef = useRef<HTMLInputElement | null>(null);

  function startEdit(groupName: string) {
    setEditing(groupName);
    setNewName(groupName);
    setRenameError(null);
    setTimeout(() => inputRef.current?.select(), 0);
  }

  function cancelEdit() {
    setEditing(null);
    setNewName("");
    setRenameError(null);
  }

  function startEditAuthor(groupName: string, currentAuthor: string | null) {
    setEditingAuthor(groupName);
    setNewAuthor(currentAuthor ?? "");
    setAuthorError(null);
    setTimeout(() => authorInputRef.current?.select(), 0);
  }

  function cancelEditAuthor() {
    setEditingAuthor(null);
    setNewAuthor("");
    setAuthorError(null);
  }

  async function confirmAuthor(groupName: string, currentAuthor: string | null) {
    const trimmed = newAuthor.trim();
    if (trimmed === (currentAuthor ?? "")) {
      cancelEditAuthor();
      return;
    }

    setSavingAuthor(true);
    setAuthorError(null);

    const response = await fetch("/api/admin/stickers/groups/author", {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ group_name: groupName, author: trimmed || null }),
    });

    setSavingAuthor(false);

    if (!response.ok) {
      const body = await response.json().catch(() => ({}));
      setAuthorError((body as { error?: string }).error ?? "Update failed");
      return;
    }

    setEditingAuthor(null);
    setNewAuthor("");
    router.refresh();
  }

  async function confirmRename(oldName: string) {
    const trimmed = newName.trim();
    if (!trimmed || trimmed === oldName) {
      cancelEdit();
      return;
    }

    setRenaming(true);
    setRenameError(null);

    const response = await fetch("/api/admin/stickers/groups/rename", {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ old_name: oldName, new_name: trimmed }),
    });

    setRenaming(false);

    if (!response.ok) {
      const body = await response.json().catch(() => ({}));
      setRenameError((body as { error?: string }).error ?? "Rename failed");
      return;
    }

    setEditing(null);
    setNewName("");
    router.refresh();
  }

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
            <div
              className="flex items-center"
              key={group.name}
              style={idx > 0 ? { borderTop: "1px solid hsl(var(--border))" } : undefined}
            >
              {/* Clickable link area */}
              <Link
                className="group flex flex-1 items-center gap-4 px-4 py-3 transition-colors hover:bg-accent"
                href={`/stickers?group=${encodeURIComponent(group.name)}`}
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
                    {group.author ? (
                      <span className="ml-1">· by {group.author}</span>
                    ) : null}
                    {group.pending > 0 ? (
                      <span className="ml-1 text-amber-500">
                        · {group.pending} pending
                      </span>
                    ) : null}
                  </p>
                </div>
                <span className="text-xs text-muted-foreground group-hover:text-foreground">
                  →
                </span>
              </Link>

              {/* Controls — outside the Link so clicks don't navigate */}
              <div className="shrink-0 flex items-center gap-1 px-3">
                {/* Author edit */}
                {editingAuthor === group.name ? (
                  <div className="flex items-center gap-1.5">
                    <div className="space-y-1">
                      <Input
                        className="h-7 w-32 text-xs"
                        disabled={savingAuthor}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") confirmAuthor(group.name, group.author);
                          if (e.key === "Escape") cancelEditAuthor();
                        }}
                        onChange={(e) => setNewAuthor(e.target.value)}
                        placeholder="Author name"
                        ref={authorInputRef}
                        type="text"
                        value={newAuthor}
                      />
                      {authorError ? (
                        <p className="text-[10px] text-destructive">{authorError}</p>
                      ) : null}
                    </div>
                    <Button
                      className="h-7 w-7"
                      disabled={savingAuthor}
                      onClick={() => confirmAuthor(group.name, group.author)}
                      size="icon"
                      type="button"
                      variant="default"
                    >
                      ✓
                    </Button>
                    <Button
                      className="h-7 w-7"
                      disabled={savingAuthor}
                      onClick={cancelEditAuthor}
                      size="icon"
                      type="button"
                      variant="ghost"
                    >
                      ✕
                    </Button>
                  </div>
                ) : (
                  <Button
                    className="h-7 px-2 text-xs text-muted-foreground"
                    onClick={() => startEditAuthor(group.name, group.author)}
                    size="sm"
                    type="button"
                    variant="ghost"
                  >
                    {group.author ? "Edit author" : "Add author"}
                  </Button>
                )}

                {/* Rename */}
                {editing === group.name ? (
                  <div className="flex items-center gap-1.5">
                    <div className="space-y-1">
                      <Input
                        className="h-7 w-36 text-xs"
                        disabled={renaming}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") confirmRename(group.name);
                          if (e.key === "Escape") cancelEdit();
                        }}
                        onChange={(e) => setNewName(e.target.value)}
                        ref={inputRef}
                        type="text"
                        value={newName}
                      />
                      {renameError ? (
                        <p className="text-[10px] text-destructive">{renameError}</p>
                      ) : null}
                    </div>
                    <Button
                      className="h-7 w-7"
                      disabled={renaming || !newName.trim()}
                      onClick={() => confirmRename(group.name)}
                      size="icon"
                      type="button"
                      variant="default"
                    >
                      ✓
                    </Button>
                    <Button
                      className="h-7 w-7"
                      disabled={renaming}
                      onClick={cancelEdit}
                      size="icon"
                      type="button"
                      variant="ghost"
                    >
                      ✕
                    </Button>
                  </div>
                ) : (
                  <Button
                    className="h-7 px-2 text-xs text-muted-foreground"
                    onClick={() => startEdit(group.name)}
                    size="sm"
                    type="button"
                    variant="ghost"
                  >
                    Rename
                  </Button>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
