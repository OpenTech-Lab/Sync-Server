ALTER TABLE moderation_reports
    DROP CONSTRAINT IF EXISTS moderation_reports_resolution_action_check;

ALTER TABLE moderation_reports
    ADD CONSTRAINT moderation_reports_resolution_action_check
    CHECK (
        resolution_action IN (
            'dismiss',
            'remove_content',
            'suspend_user',
            'remove_content_and_suspend_user'
        )
    );

ALTER TABLE users
    DROP COLUMN IF EXISTS safety_warning_count,
    DROP COLUMN IF EXISTS dm_restricted_until;
