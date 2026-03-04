ALTER TABLE users
ADD COLUMN IF NOT EXISTS message_public_key TEXT;
