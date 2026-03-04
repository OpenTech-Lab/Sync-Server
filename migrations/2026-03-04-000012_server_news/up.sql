CREATE TABLE server_news (
    id UUID PRIMARY KEY,
    title TEXT NOT NULL,
    summary TEXT,
    markdown_content TEXT NOT NULL,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    published_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_server_news_published_at ON server_news (published_at DESC);
