DROP INDEX IF EXISTS idx_guild_score_events_event_type;
DROP INDEX IF EXISTS idx_guild_score_events_user_created_at;
DROP TABLE IF EXISTS guild_score_events;

ALTER TABLE user_guild_stats
    DROP COLUMN IF EXISTS derived_rank,
    DROP COLUMN IF EXISTS derived_level;
