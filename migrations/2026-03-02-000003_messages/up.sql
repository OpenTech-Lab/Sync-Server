CREATE TABLE messages (
    id           UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    sender_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    recipient_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    content      TEXT NOT NULL,
    delivered_at TIMESTAMPTZ,
    read_at      TIMESTAMPTZ,
    deleted_at   TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Composite index for conversation pagination (keyset on created_at DESC, id DESC)
CREATE INDEX idx_messages_conversation ON messages (
    LEAST(sender_id::text, recipient_id::text),
    GREATEST(sender_id::text, recipient_id::text),
    created_at DESC
);
