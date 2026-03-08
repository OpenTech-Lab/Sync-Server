CREATE TABLE user_guild_stats (
    user_id                  UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    active_days              INT NOT NULL DEFAULT 0 CHECK (active_days >= 0),
    contribution_score       INT NOT NULL DEFAULT 0 CHECK (contribution_score >= 0),
    last_active_day          DATE NULL,
    automation_review_state  TEXT NOT NULL DEFAULT 'clear'
                                 CHECK (automation_review_state IN ('clear', 'challenged', 'frozen')),
    created_at               TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TRIGGER user_guild_stats_set_updated_at
    BEFORE UPDATE ON user_guild_stats
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

CREATE TABLE daily_action_counters (
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    action_key  TEXT NOT NULL,
    day_bucket  DATE NOT NULL,
    count       INT NOT NULL DEFAULT 0 CHECK (count >= 0),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, action_key, day_bucket)
);

CREATE INDEX idx_daily_action_counters_action_day
    ON daily_action_counters (action_key, day_bucket DESC);

CREATE TRIGGER daily_action_counters_set_updated_at
    BEFORE UPDATE ON daily_action_counters
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();
