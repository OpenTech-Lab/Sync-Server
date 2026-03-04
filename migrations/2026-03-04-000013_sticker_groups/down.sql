DROP INDEX IF EXISTS idx_stickers_group_name;

ALTER TABLE stickers
DROP COLUMN IF EXISTS group_name;
