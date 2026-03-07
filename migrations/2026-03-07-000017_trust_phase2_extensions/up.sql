ALTER TABLE user_trust_stats
    ADD COLUMN derived_level INT NOT NULL DEFAULT 1 CHECK (derived_level BETWEEN 1 AND 10),
    ADD COLUMN derived_rank TEXT NOT NULL DEFAULT 'F'
        CHECK (derived_rank IN ('F', 'E', 'D', 'C', 'B', 'A', 'S'));

CREATE TABLE trust_score_events (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    granter_user_id UUID NULL REFERENCES users(id) ON DELETE SET NULL,
    event_type      TEXT NOT NULL,
    delta           INT NOT NULL,
    reference_id    TEXT NULL,
    metadata        JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_trust_score_events_user_created_at
    ON trust_score_events (user_id, created_at DESC);

CREATE INDEX idx_trust_score_events_event_type
    ON trust_score_events (event_type, created_at DESC);
