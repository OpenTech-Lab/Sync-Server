DROP INDEX IF EXISTS idx_messages_recipient;
DROP INDEX IF EXISTS idx_messages_recipient_unread;
DROP INDEX IF EXISTS idx_messages_sender_recipient_created;

CREATE INDEX idx_messages_conversation ON messages (
    LEAST(sender_id::text, recipient_id::text),
    GREATEST(sender_id::text, recipient_id::text),
    created_at DESC
);
