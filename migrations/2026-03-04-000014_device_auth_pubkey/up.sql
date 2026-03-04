ALTER TABLE users
    ADD COLUMN device_auth_pubkey TEXT;

CREATE UNIQUE INDEX idx_users_device_auth_pubkey
    ON users (device_auth_pubkey)
    WHERE device_auth_pubkey IS NOT NULL;
