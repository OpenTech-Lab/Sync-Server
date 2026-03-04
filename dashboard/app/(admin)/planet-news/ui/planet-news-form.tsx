"use client";

import { useMemo, useState } from "react";

import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { Textarea } from "@/components/ui/textarea";
import { renderSimpleMarkdownToHtml } from "@/lib/simple-markdown";

type ServerNewsItem = {
  id: string;
  title: string;
  summary: string | null;
  markdown_content: string;
  created_by: string | null;
  published_at: string;
  updated_at: string;
};

export function PlanetNewsForm({ initialNews }: { initialNews: ServerNewsItem[] }) {
  const defaultMarkdown = "# Update\n\nWrite your announcement here.";
  const [title, setTitle] = useState("");
  const [summary, setSummary] = useState("");
  const [markdown, setMarkdown] = useState(defaultMarkdown);
  const [items, setItems] = useState(initialNews);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [removingId, setRemovingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const previewHtml = useMemo(() => renderSimpleMarkdownToHtml(markdown), [markdown]);
  const isEditing = editingId !== null;

  function resetForm() {
    setTitle("");
    setSummary("");
    setMarkdown(defaultMarkdown);
    setEditingId(null);
  }

  async function onSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSaving(true);
    setError(null);

    try {
      const response = await fetch(
        editingId ? `/api/admin/server-news/${editingId}` : "/api/admin/server-news",
        {
          method: editingId ? "PUT" : "POST",
          headers: {
            "Content-Type": "application/json",
          },
          body: JSON.stringify({
            title,
            summary: summary.trim() || null,
            markdown_content: markdown,
          }),
        },
      );

      if (!response.ok) {
        const body = (await response.json().catch(() => null)) as
          | { error?: string }
          | null;
        setError(
          body?.error ?? (editingId ? "Failed to update post" : "Failed to publish post"),
        );
        return;
      }

      const saved = (await response.json()) as ServerNewsItem;
      if (editingId) {
        setItems((prev) => prev.map((item) => (item.id === editingId ? saved : item)));
      } else {
        setItems((prev) => [saved, ...prev]);
      }
      resetForm();
    } catch {
      setError(editingId ? "Failed to update post" : "Failed to publish post");
    } finally {
      setSaving(false);
    }
  }

  async function onRemove(item: ServerNewsItem) {
    if (!confirm(`Remove "${item.title}"? This cannot be undone.`)) {
      return;
    }
    setRemovingId(item.id);
    setError(null);

    try {
      const response = await fetch(`/api/admin/server-news/${item.id}`, {
        method: "DELETE",
      });

      if (!response.ok) {
        const body = (await response.json().catch(() => null)) as
          | { error?: string }
          | null;
        setError(body?.error ?? "Failed to remove post");
        return;
      }

      setItems((prev) => prev.filter((value) => value.id !== item.id));
      if (editingId === item.id) {
        resetForm();
      }
    } catch {
      setError("Failed to remove post");
    } finally {
      setRemovingId(null);
    }
  }

  function onEdit(item: ServerNewsItem) {
    setEditingId(item.id);
    setTitle(item.title);
    setSummary(item.summary ?? "");
    setMarkdown(item.markdown_content);
    setError(null);
  }

  return (
    <div className="space-y-8">
      <section className="space-y-4">
        <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
          {isEditing ? "Edit post" : "New post"}
        </p>
        <form className="space-y-4" onSubmit={onSubmit}>
          <div className="grid gap-4 sm:grid-cols-2">
            <div className="space-y-2">
              <Label htmlFor="news-title">Title</Label>
              <Input
                id="news-title"
                maxLength={120}
                onChange={(event) => setTitle(event.target.value)}
                placeholder="Server maintenance window"
                required
                type="text"
                value={title}
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="news-summary">Summary <span className="text-muted-foreground font-normal">(optional)</span></Label>
              <Input
                id="news-summary"
                maxLength={280}
                onChange={(event) => setSummary(event.target.value)}
                placeholder="Short summary shown in list cards"
                type="text"
                value={summary}
              />
            </div>
          </div>

          <div className="space-y-2">
            <Label htmlFor="news-markdown">Markdown content</Label>
            <Textarea
              className="min-h-52 font-mono text-xs"
              id="news-markdown"
              maxLength={20000}
              onChange={(event) => setMarkdown(event.target.value)}
              required
              value={markdown}
            />
          </div>

          <div className="space-y-2 rounded-lg border bg-muted/20 p-4">
            <p className="text-[11px] font-semibold tracking-widest text-muted-foreground/70 uppercase">
              Preview
            </p>
            <article
              className="space-y-2 text-sm [&_a]:text-primary [&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_h1]:text-lg [&_h1]:font-semibold [&_h2]:text-base [&_h2]:font-semibold [&_h3]:text-sm [&_h3]:font-semibold [&_li]:ml-5 [&_li]:list-disc [&_p]:leading-6"
              dangerouslySetInnerHTML={{ __html: previewHtml }}
            />
          </div>

          {error ? (
            <Alert variant="destructive">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          ) : null}

          <div className="flex flex-wrap gap-2">
            <Button disabled={saving} type="submit">
              {saving
                ? isEditing
                  ? "Saving…"
                  : "Publishing…"
                : isEditing
                  ? "Save changes"
                  : "Publish post"}
            </Button>
            {isEditing ? (
              <Button
                disabled={saving}
                onClick={() => resetForm()}
                type="button"
                variant="outline"
              >
                Cancel
              </Button>
            ) : null}
          </div>
        </form>
      </section>

      <Separator />

      <section className="space-y-4">
        <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
          Published posts{" "}
          <span className="normal-case font-normal text-muted-foreground/50">({items.length})</span>
        </p>

        {items.length === 0 ? (
          <p className="text-sm text-muted-foreground">No posts published yet.</p>
        ) : (
          <div className="divide-y rounded-lg border">
            {items.map((item) => {
              const html = renderSimpleMarkdownToHtml(item.markdown_content);
              return (
                <div className="px-4 py-4" key={item.id}>
                  <div className="flex items-start justify-between gap-4">
                    <div className="min-w-0">
                      <p className="font-medium leading-tight">{item.title}</p>
                      {item.summary ? (
                        <p className="mt-0.5 text-sm text-muted-foreground">{item.summary}</p>
                      ) : null}
                      <p className="mt-1 text-xs text-muted-foreground/60">
                        {new Date(item.published_at).toLocaleString()}
                      </p>
                    </div>
                    <div className="flex shrink-0 gap-2">
                      <Button
                        disabled={saving || removingId === item.id}
                        onClick={() => onEdit(item)}
                        size="sm"
                        type="button"
                        variant="outline"
                      >
                        Edit
                      </Button>
                      <Button
                        disabled={saving || removingId === item.id}
                        onClick={() => void onRemove(item)}
                        size="sm"
                        type="button"
                        variant="destructive"
                      >
                        {removingId === item.id ? "Removing…" : "Remove"}
                      </Button>
                    </div>
                  </div>
                  <article
                    className="mt-3 space-y-2 text-sm text-muted-foreground [&_a]:text-primary [&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_h1]:text-lg [&_h1]:font-semibold [&_h1]:text-foreground [&_h2]:text-base [&_h2]:font-semibold [&_h2]:text-foreground [&_h3]:text-sm [&_h3]:font-semibold [&_h3]:text-foreground [&_li]:ml-5 [&_li]:list-disc [&_p]:leading-6"
                    dangerouslySetInnerHTML={{ __html: html }}
                  />
                </div>
              );
            })}
          </div>
        )}
      </section>
    </div>
  );
}
