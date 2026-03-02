CREATE TABLE IF NOT EXISTS stickers (
  id UUID PRIMARY KEY,
  uploader_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  mime_type TEXT NOT NULL,
  content_base64 TEXT NOT NULL,
  size_bytes INTEGER NOT NULL CHECK (size_bytes > 0),
  status TEXT NOT NULL CHECK (status IN ('active', 'pending', 'rejected')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_stickers_uploader_id ON stickers(uploader_id);
CREATE INDEX IF NOT EXISTS idx_stickers_status ON stickers(status);
