-- idx_messages_conversation indexed LEAST/GREATEST expressions that no query
-- uses, so every conversation, unread and mark-read query scanned the table.
DROP INDEX IF EXISTS idx_messages_conversation;

-- Conversation history and catch-up: each direction of the pair is one lookup.
CREATE INDEX idx_messages_sender_recipient_created
    ON messages (sender_id, recipient_id, created_at DESC, id DESC);

-- Unread counts for a recipient.
CREATE INDEX idx_messages_recipient_unread
    ON messages (recipient_id, sender_id)
    WHERE read_at IS NULL AND deleted_at IS NULL;

-- ON DELETE CASCADE from users(id) when an account is deleted.
CREATE INDEX idx_messages_recipient ON messages (recipient_id);
