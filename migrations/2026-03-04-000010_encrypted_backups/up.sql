CREATE TABLE IF NOT EXISTS encrypted_backups (
  user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
  encrypted_blob TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TRIGGER encrypted_backups_set_updated_at
  BEFORE UPDATE ON encrypted_backups
  FOR EACH ROW
  EXECUTE FUNCTION set_updated_at();
