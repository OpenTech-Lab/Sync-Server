CREATE TABLE admin_settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE admin_audit_logs (
    id            UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    actor_user_id UUID NULL REFERENCES users(id) ON DELETE SET NULL,
    action        TEXT NOT NULL,
    target        TEXT NULL,
    details       JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_admin_audit_logs_created_at ON admin_audit_logs (created_at DESC);