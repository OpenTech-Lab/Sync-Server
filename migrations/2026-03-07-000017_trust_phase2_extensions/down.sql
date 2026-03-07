DROP INDEX IF EXISTS idx_trust_score_events_event_type;
DROP INDEX IF EXISTS idx_trust_score_events_user_created_at;
DROP TABLE IF EXISTS trust_score_events;

ALTER TABLE user_trust_stats
    DROP COLUMN IF EXISTS derived_rank,
    DROP COLUMN IF EXISTS derived_level;
