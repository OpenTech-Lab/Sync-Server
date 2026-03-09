DO $$
BEGIN
    IF to_regclass('public.user_guild_stats') IS NOT NULL
       AND to_regclass('public.user_trust_stats') IS NULL THEN
        EXECUTE 'ALTER TABLE user_guild_stats RENAME TO user_trust_stats';
    END IF;

    IF to_regclass('public.guild_score_events') IS NOT NULL
       AND to_regclass('public.trust_score_events') IS NULL THEN
        EXECUTE 'ALTER TABLE guild_score_events RENAME TO trust_score_events';
    END IF;

    IF to_regclass('public.user_trust_stats') IS NOT NULL
       AND EXISTS (
           SELECT 1
           FROM pg_trigger t
           JOIN pg_class c ON c.oid = t.tgrelid
           JOIN pg_namespace n ON n.oid = c.relnamespace
           WHERE n.nspname = 'public'
             AND c.relname = 'user_trust_stats'
             AND t.tgname = 'user_guild_stats_set_updated_at'
       )
       AND NOT EXISTS (
           SELECT 1
           FROM pg_trigger t
           JOIN pg_class c ON c.oid = t.tgrelid
           JOIN pg_namespace n ON n.oid = c.relnamespace
           WHERE n.nspname = 'public'
             AND c.relname = 'user_trust_stats'
             AND t.tgname = 'user_trust_stats_set_updated_at'
       ) THEN
        EXECUTE
            'ALTER TRIGGER user_guild_stats_set_updated_at ON user_trust_stats
             RENAME TO user_trust_stats_set_updated_at';
    END IF;

    IF to_regclass('public.idx_guild_score_events_user_created_at') IS NOT NULL
       AND to_regclass('public.idx_trust_score_events_user_created_at') IS NULL THEN
        EXECUTE
            'ALTER INDEX idx_guild_score_events_user_created_at
             RENAME TO idx_trust_score_events_user_created_at';
    END IF;

    IF to_regclass('public.idx_guild_score_events_event_type') IS NOT NULL
       AND to_regclass('public.idx_trust_score_events_event_type') IS NULL THEN
        EXECUTE
            'ALTER INDEX idx_guild_score_events_event_type
             RENAME TO idx_trust_score_events_event_type';
    END IF;
END $$;
