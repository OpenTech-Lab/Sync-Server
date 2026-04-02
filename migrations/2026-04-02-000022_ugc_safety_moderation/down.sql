DROP TABLE IF EXISTS user_blocks;
DROP TRIGGER IF EXISTS moderation_reports_set_updated_at ON moderation_reports;
DROP TABLE IF EXISTS moderation_reports;

ALTER TABLE room_messages
    DROP COLUMN IF EXISTS deleted_at;

ALTER TABLE users
    DROP COLUMN IF EXISTS ugc_terms_version,
    DROP COLUMN IF EXISTS ugc_terms_accepted_at;
