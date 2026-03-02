CREATE TABLE federation_actor_keys (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    actor_username TEXT NOT NULL UNIQUE REFERENCES users(username) ON DELETE CASCADE,
    key_id TEXT NOT NULL UNIQUE,
    public_key_pem TEXT NOT NULL,
    private_key_pkcs8 TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE federation_inbox_activities (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    activity_id TEXT NOT NULL UNIQUE,
    actor TEXT NOT NULL,
    recipient_username TEXT NOT NULL REFERENCES users(username) ON DELETE CASCADE,
    activity_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE federation_deliveries (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    activity_id TEXT NOT NULL,
    sender_username TEXT NOT NULL REFERENCES users(username) ON DELETE CASCADE,
    destination TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'delivered', 'failed', 'dead_letter')),
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    next_attempt_at TIMESTAMPTZ,
    delivered_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(activity_id, destination)
);

CREATE TABLE federation_remote_messages (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    activity_id TEXT NOT NULL UNIQUE REFERENCES federation_inbox_activities(activity_id) ON DELETE CASCADE,
    object_id TEXT,
    actor TEXT NOT NULL,
    recipient_username TEXT NOT NULL REFERENCES users(username) ON DELETE CASCADE,
    content TEXT NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_federation_inbox_actor ON federation_inbox_activities (actor);
CREATE INDEX idx_federation_deliveries_status ON federation_deliveries (status);
CREATE INDEX idx_federation_deliveries_next_attempt ON federation_deliveries (next_attempt_at);
CREATE INDEX idx_federation_remote_messages_recipient ON federation_remote_messages (recipient_username, received_at DESC);

CREATE TRIGGER federation_deliveries_set_updated_at
    BEFORE UPDATE ON federation_deliveries
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();
