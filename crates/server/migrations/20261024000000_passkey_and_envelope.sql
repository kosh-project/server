ALTER TABLE users
ADD COLUMN wrapped_vault_key BLOB NOT NULL DEFAULT x'';

CREATE TABLE IF NOT EXISTS webauthn_credentials (
    credential_id BLOB PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    encrypted_metadata BLOB NOT NULL DEFAULT x'' CHECK (length(encrypted_metadata) <= 1024),
    passkey_data BLOB NOT NULL,
    counter INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    last_used INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_webauthn_credentials_user
ON webauthn_credentials(user_id);
