ALTER TABLE users
    ADD COLUMN ugc_terms_accepted_at TIMESTAMPTZ,
    ADD COLUMN ugc_terms_version INT4 NOT NULL DEFAULT 0;

ALTER TABLE room_messages
    ADD COLUMN deleted_at TIMESTAMPTZ;

CREATE TABLE moderation_reports (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    reporter_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    reported_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source TEXT NOT NULL CHECK (source IN ('user_report', 'block_action')),
    content_kind TEXT NOT NULL CHECK (content_kind IN ('user_profile', 'direct_message', 'room_message')),
    content_id TEXT,
    reason_code TEXT NOT NULL,
    reporter_note TEXT,
    content_excerpt TEXT,
    status TEXT NOT NULL DEFAULT 'open'
        CHECK (status IN ('open', 'resolved', 'dismissed')),
    resolution_action TEXT
        CHECK (resolution_action IN ('dismiss', 'remove_content', 'suspend_user', 'remove_content_and_suspend_user')),
    resolution_notes TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    review_due_at TIMESTAMPTZ NOT NULL DEFAULT (NOW() + INTERVAL '24 hours'),
    reviewed_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    reviewed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_moderation_reports_status_created
    ON moderation_reports (status, created_at DESC);
CREATE INDEX idx_moderation_reports_due
    ON moderation_reports (review_due_at);
CREATE INDEX idx_moderation_reports_reported_user
    ON moderation_reports (reported_user_id, created_at DESC);

CREATE TRIGGER moderation_reports_set_updated_at
    BEFORE UPDATE ON moderation_reports
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

CREATE TABLE user_blocks (
    blocker_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    blocked_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    report_id UUID REFERENCES moderation_reports(id) ON DELETE SET NULL,
    reason_code TEXT,
    reporter_note TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (blocker_user_id, blocked_user_id),
    CHECK (blocker_user_id <> blocked_user_id)
);

CREATE INDEX idx_user_blocks_blocked_user_id
    ON user_blocks (blocked_user_id);
