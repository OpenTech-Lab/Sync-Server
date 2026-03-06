-- Users pending manual admin approval get is_approved = FALSE.
-- Existing users are automatically approved (DEFAULT TRUE).
ALTER TABLE users ADD COLUMN is_approved BOOLEAN NOT NULL DEFAULT TRUE;
