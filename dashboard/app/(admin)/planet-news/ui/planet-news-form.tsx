"use client";

import { useMemo, useState } from "react";

import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
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
    <div className="space-y-4">
      <Card className="py-0">
        <CardHeader>
          <CardTitle>{isEditing ? "Edit post" : "Publish post"}</CardTitle>
          <CardDescription>
            Use markdown for headings, lists, links, and inline code.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form className="space-y-4" onSubmit={onSubmit}>
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
              <Label htmlFor="news-summary">Summary</Label>
              <Input
                id="news-summary"
                maxLength={280}
                onChange={(event) => setSummary(event.target.value)}
                placeholder="Optional short summary shown in list cards"
                type="text"
                value={summary}
              />
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

            <div className="space-y-2 rounded-lg border bg-muted/20 p-3">
              <p className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
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
                    ? "Saving..."
                    : "Publishing..."
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
                  Cancel edit
                </Button>
              ) : null}
            </div>
          </form>
        </CardContent>
      </Card>

      <section className="space-y-3">
        <p className="text-sm font-medium">Published posts ({items.length})</p>
        {items.length === 0 ? (
          <Card className="py-0">
            <CardContent className="py-4 text-sm text-muted-foreground">
              No server news published yet.
            </CardContent>
          </Card>
        ) : (
          items.map((item) => {
            const html = renderSimpleMarkdownToHtml(item.markdown_content);
            return (
              <Card className="py-0" key={item.id}>
                <CardHeader>
                  <div className="flex items-start justify-between gap-3">
                    <div>
                      <CardTitle className="text-base">{item.title}</CardTitle>
                      {item.summary ? (
                        <CardDescription className="mt-2">{item.summary}</CardDescription>
                      ) : null}
                    </div>
                    <div className="flex shrink-0 items-center gap-2">
                      <span className="text-xs text-muted-foreground">
                        {new Date(item.published_at).toLocaleString()}
                      </span>
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
                        {removingId === item.id ? "Removing..." : "Remove"}
                      </Button>
                    </div>
                  </div>
                </CardHeader>
                <CardContent>
                  <article
                    className="space-y-2 text-sm [&_a]:text-primary [&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_h1]:text-lg [&_h1]:font-semibold [&_h2]:text-base [&_h2]:font-semibold [&_h3]:text-sm [&_h3]:font-semibold [&_li]:ml-5 [&_li]:list-disc [&_p]:leading-6"
                    dangerouslySetInnerHTML={{ __html: html }}
                  />
                </CardContent>
              </Card>
            );
          })
        )}
      </section>
    </div>
  );
}
