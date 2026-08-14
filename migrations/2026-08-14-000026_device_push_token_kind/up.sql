ALTER TABLE device_push_tokens
  ADD COLUMN token_kind TEXT NOT NULL DEFAULT 'default' CHECK (token_kind IN ('default', 'voip'));

CREATE INDEX IF NOT EXISTS idx_device_push_tokens_user_id_token_kind ON device_push_tokens(user_id, token_kind);
