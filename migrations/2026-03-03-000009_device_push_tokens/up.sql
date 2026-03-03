CREATE TABLE IF NOT EXISTS device_push_tokens (
  id UUID PRIMARY KEY,
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  platform TEXT NOT NULL CHECK (platform IN ('ios', 'android', 'macos', 'windows', 'linux', 'web', 'unknown')),
  token TEXT NOT NULL UNIQUE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_seen_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_device_push_tokens_user_id ON device_push_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_device_push_tokens_platform ON device_push_tokens(platform);
