ALTER TABLE user_trust_stats
    ADD COLUMN last_human_activity_at TIMESTAMPTZ NULL,
    ADD COLUMN suspicious_activity_streak INT NOT NULL DEFAULT 0
        CHECK (suspicious_activity_streak >= 0);
