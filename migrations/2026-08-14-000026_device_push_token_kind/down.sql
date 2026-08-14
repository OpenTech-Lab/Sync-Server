DROP INDEX IF EXISTS idx_device_push_tokens_user_id_token_kind;
ALTER TABLE device_push_tokens DROP COLUMN IF EXISTS token_kind;
