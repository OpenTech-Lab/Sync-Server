"use client";

import { useMemo, useState } from "react";

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
  const [title, setTitle] = useState("");
  const [summary, setSummary] = useState("");
  const [markdown, setMarkdown] = useState("# Update\n\nWrite your announcement here.");
  const [items, setItems] = useState(initialNews);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const previewHtml = useMemo(() => renderSimpleMarkdownToHtml(markdown), [markdown]);

  async function onSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSaving(true);
    setError(null);

    const response = await fetch("/api/admin/server-news", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        title,
        summary: summary.trim() || null,
        markdown_content: markdown,
      }),
    });

    if (!response.ok) {
      const body = (await response.json().catch(() => null)) as
        | { error?: string }
        | null;
      setError(body?.error ?? "Failed to publish post");
      setSaving(false);
      return;
    }

    const created = (await response.json()) as ServerNewsItem;
    setItems((prev) => [created, ...prev]);
    setTitle("");
    setSummary("");
    setMarkdown("# Update\n\nWrite your announcement here.");
    setSaving(false);
  }

  return (
    <div className="space-y-4">
      <form className="space-y-4 rounded-lg border bg-background p-4" onSubmit={onSubmit}>
        <label className="block text-sm">
          <span className="mb-1 block text-muted-foreground">Title</span>
          <input
            className="w-full rounded-md border px-3 py-2"
            maxLength={120}
            onChange={(event) => setTitle(event.target.value)}
            placeholder="Server maintenance window"
            required
            type="text"
            value={title}
          />
        </label>

        <label className="block text-sm">
          <span className="mb-1 block text-muted-foreground">Summary</span>
          <input
            className="w-full rounded-md border px-3 py-2"
            maxLength={280}
            onChange={(event) => setSummary(event.target.value)}
            placeholder="Optional short summary shown in list cards"
            type="text"
            value={summary}
          />
        </label>

        <label className="block text-sm">
          <span className="mb-1 block text-muted-foreground">Markdown content</span>
          <textarea
            className="min-h-48 w-full rounded-md border px-3 py-2 font-mono text-xs"
            maxLength={20000}
            onChange={(event) => setMarkdown(event.target.value)}
            required
            value={markdown}
          />
        </label>

        <div className="space-y-2 rounded-lg border bg-muted/20 p-3">
          <p className="text-xs uppercase tracking-wide text-muted-foreground">Preview</p>
          <article
            className="space-y-2 text-sm [&_a]:text-primary [&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_h1]:text-lg [&_h1]:font-semibold [&_h2]:text-base [&_h2]:font-semibold [&_h3]:text-sm [&_h3]:font-semibold [&_li]:ml-5 [&_li]:list-disc [&_p]:leading-6"
            dangerouslySetInnerHTML={{ __html: previewHtml }}
          />
        </div>

        {error ? <p className="text-sm text-destructive">{error}</p> : null}

        <button
          className="rounded-md bg-primary px-4 py-2 text-primary-foreground disabled:opacity-70"
          disabled={saving}
          type="submit"
        >
          {saving ? "Publishing..." : "Publish post"}
        </button>
      </form>

      <section className="space-y-3">
        <p className="text-sm font-medium">Published posts ({items.length})</p>
        {items.length === 0 ? (
          <p className="rounded-lg border bg-background p-3 text-sm text-muted-foreground">
            No server news published yet.
          </p>
        ) : (
          items.map((item) => {
            const html = renderSimpleMarkdownToHtml(item.markdown_content);
            return (
              <article className="rounded-lg border bg-background p-4" key={item.id}>
                <div className="mb-2 flex items-start justify-between gap-3">
                  <h2 className="text-base font-semibold">{item.title}</h2>
                  <span className="shrink-0 text-xs text-muted-foreground">
                    {new Date(item.published_at).toLocaleString()}
                  </span>
                </div>
                {item.summary ? (
                  <p className="mb-3 text-sm text-muted-foreground">{item.summary}</p>
                ) : null}
                <article
                  className="space-y-2 text-sm [&_a]:text-primary [&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_h1]:text-lg [&_h1]:font-semibold [&_h2]:text-base [&_h2]:font-semibold [&_h3]:text-sm [&_h3]:font-semibold [&_li]:ml-5 [&_li]:list-disc [&_p]:leading-6"
                  dangerouslySetInnerHTML={{ __html: html }}
                />
              </article>
            );
          })
        )}
      </section>
    </div>
  );
}
