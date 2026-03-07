DROP TRIGGER IF EXISTS daily_action_counters_set_updated_at ON daily_action_counters;
DROP INDEX IF EXISTS idx_daily_action_counters_action_day;
DROP TABLE IF EXISTS daily_action_counters;

DROP TRIGGER IF EXISTS user_trust_stats_set_updated_at ON user_trust_stats;
DROP TABLE IF EXISTS user_trust_stats;
