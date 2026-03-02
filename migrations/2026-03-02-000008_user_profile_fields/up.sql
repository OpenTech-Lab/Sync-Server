ALTER TABLE users
    ADD COLUMN avatar_base64 TEXT;

ALTER TABLE users
    ADD CONSTRAINT users_avatar_base64_len_chk
    CHECK (avatar_base64 IS NULL OR char_length(avatar_base64) <= 400000);
