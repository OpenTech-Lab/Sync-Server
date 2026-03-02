-- Development seed data — DO NOT run in production.
--
-- Inserts a default admin user for local testing.
-- The password hash below is for the value 'admintest123' using bcrypt cost 12.
-- To regenerate: cargo run --example hash_password -- admintest123
--
-- Usage: psql $DATABASE_URL -f scripts/seed.sql

INSERT INTO users (username, email, password_hash, role)
VALUES (
    'admin',
    'admin@localhost',
    -- bcrypt hash of 'admintest123' (cost 12) — REPLACE before real use
    '$2b$12$XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX',
    'admin'
)
ON CONFLICT (username) DO NOTHING;
