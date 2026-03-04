ALTER TABLE stickers
ADD COLUMN group_name TEXT NOT NULL DEFAULT 'General';

CREATE INDEX IF NOT EXISTS idx_stickers_group_name ON stickers(group_name);
