ALTER TABLE users
    ADD COLUMN dm_restricted_until TIMESTAMPTZ,
    ADD COLUMN safety_warning_count INT4 NOT NULL DEFAULT 0;

ALTER TABLE moderation_reports
    DROP CONSTRAINT IF EXISTS moderation_reports_resolution_action_check;

ALTER TABLE moderation_reports
    ADD CONSTRAINT moderation_reports_resolution_action_check
    CHECK (
        resolution_action IN (
            'dismiss',
            'remove_content',
            'remove_content_and_limit_new_direct_messages',
            'suspend_user',
            'remove_content_and_suspend_user'
        )
    );
