ALTER TABLE user_trust_stats RENAME TO user_guild_stats;
ALTER TABLE trust_score_events RENAME TO guild_score_events;

ALTER TRIGGER user_trust_stats_set_updated_at ON user_guild_stats
    RENAME TO user_guild_stats_set_updated_at;

ALTER INDEX idx_trust_score_events_user_created_at
    RENAME TO idx_guild_score_events_user_created_at;
ALTER INDEX idx_trust_score_events_event_type
    RENAME TO idx_guild_score_events_event_type;
