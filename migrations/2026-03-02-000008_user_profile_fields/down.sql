ALTER TABLE users
    DROP CONSTRAINT IF EXISTS users_avatar_base64_len_chk;

ALTER TABLE users
    DROP COLUMN IF EXISTS avatar_base64;
