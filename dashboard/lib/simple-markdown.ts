function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function safeLink(url: string): string {
  const trimmed = url.trim();
  if (trimmed.startsWith("https://") || trimmed.startsWith("http://")) {
    return trimmed;
  }
  return "#";
}

function inlineMarkdown(line: string): string {
  let output = escapeHtml(line);
  output = output.replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>");
  output = output.replace(/\*(.+?)\*/g, "<em>$1</em>");
  output = output.replace(/`([^`]+)`/g, "<code>$1</code>");
  output = output.replace(/\[(.+?)\]\((.+?)\)/g, (_match, label: string, url: string) => {
    const href = safeLink(url);
    return `<a href=\"${escapeHtml(href)}\" target=\"_blank\" rel=\"noopener noreferrer\">${label}</a>`;
  });
  return output;
}

export function renderSimpleMarkdownToHtml(markdown: string): string {
  const lines = markdown.replaceAll("\r\n", "\n").split("\n");
  const html: string[] = [];
  let inList = false;

  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed.length === 0) {
      if (inList) {
        html.push("</ul>");
        inList = false;
      }
      continue;
    }

    if (trimmed.startsWith("- ")) {
      if (!inList) {
        html.push("<ul>");
        inList = true;
      }
      html.push(`<li>${inlineMarkdown(trimmed.slice(2).trim())}</li>`);
      continue;
    }

    if (inList) {
      html.push("</ul>");
      inList = false;
    }

    if (trimmed.startsWith("### ")) {
      html.push(`<h3>${inlineMarkdown(trimmed.slice(4).trim())}</h3>`);
      continue;
    }
    if (trimmed.startsWith("## ")) {
      html.push(`<h2>${inlineMarkdown(trimmed.slice(3).trim())}</h2>`);
      continue;
    }
    if (trimmed.startsWith("# ")) {
      html.push(`<h1>${inlineMarkdown(trimmed.slice(2).trim())}</h1>`);
      continue;
    }

    html.push(`<p>${inlineMarkdown(trimmed)}</p>`);
  }

  if (inList) {
    html.push("</ul>");
  }

  return html.join("\n");
}
