-- Initial schema: users table
-- Phase 2 will add: messages, sessions, federation_actors, encrypted_backups

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE users (
    id           UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    username     TEXT        NOT NULL UNIQUE,
    email        TEXT        NOT NULL UNIQUE,
    password_hash TEXT        NOT NULL,
    role         TEXT        NOT NULL DEFAULT 'user'
                              CHECK (role IN ('admin', 'user', 'moderator')),
    is_active    BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_users_email    ON users (email);
CREATE INDEX idx_users_username ON users (username);

-- Automatically keep updated_at current on any UPDATE
CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER users_set_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();
