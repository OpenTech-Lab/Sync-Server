DROP INDEX IF EXISTS idx_users_device_auth_pubkey;

ALTER TABLE users
    DROP COLUMN IF EXISTS device_auth_pubkey;
