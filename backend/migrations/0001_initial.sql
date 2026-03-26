PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS subjects (
    id TEXT PRIMARY KEY,
    subject_type TEXT NOT NULL,
    status TEXT NOT NULL,
    display_name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS subject_identifiers (
    id TEXT PRIMARY KEY,
    subject_id TEXT NOT NULL,
    identifier_kind TEXT NOT NULL,
    identifier_value TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    FOREIGN KEY (subject_id) REFERENCES subjects(id)
);

CREATE TABLE IF NOT EXISTS credentials (
    id TEXT PRIMARY KEY,
    subject_id TEXT NOT NULL,
    credential_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'ACTIVE',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (subject_id) REFERENCES subjects(id)
);

CREATE TABLE IF NOT EXISTS password_credentials (
    subject_id TEXT PRIMARY KEY,
    password_hash TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (subject_id) REFERENCES subjects(id)
);

CREATE TABLE IF NOT EXISTS otp_identities (
    id TEXT PRIMARY KEY,
    subject_id TEXT NOT NULL,
    channel_type TEXT NOT NULL,
    channel_value TEXT NOT NULL UNIQUE,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    FOREIGN KEY (subject_id) REFERENCES subjects(id)
);

CREATE TABLE IF NOT EXISTS passkey_credentials (
    id TEXT PRIMARY KEY,
    subject_id TEXT NOT NULL,
    credential_id TEXT NOT NULL UNIQUE,
    credential_public_key TEXT NOT NULL DEFAULT '',
    attestation_object TEXT NOT NULL,
    client_data_json TEXT NOT NULL,
    transports_json TEXT NOT NULL DEFAULT '[]',
    authenticator_attachment TEXT,
    authenticator_label TEXT NOT NULL,
    sign_count INTEGER NOT NULL DEFAULT 0,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    FOREIGN KEY (subject_id) REFERENCES subjects(id)
);

CREATE TABLE IF NOT EXISTS devices (
    id TEXT PRIMARY KEY,
    subject_id TEXT NOT NULL,
    label TEXT NOT NULL,
    user_agent TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    FOREIGN KEY (subject_id) REFERENCES subjects(id)
);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    subject_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    refresh_token_hash TEXT NOT NULL,
    login_method TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    FOREIGN KEY (subject_id) REFERENCES subjects(id),
    FOREIGN KEY (device_id) REFERENCES devices(id)
);

CREATE TABLE IF NOT EXISTS audit_logs (
    id TEXT PRIMARY KEY,
    subject_id TEXT,
    subject_type TEXT,
    identifier TEXT,
    event_type TEXT NOT NULL,
    details_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_subject_identifiers_value
    ON subject_identifiers(identifier_value);

CREATE INDEX IF NOT EXISTS idx_otp_identities_value
    ON otp_identities(channel_value);

CREATE INDEX IF NOT EXISTS idx_sessions_subject_status
    ON sessions(subject_id, status);

CREATE INDEX IF NOT EXISTS idx_sessions_refresh_hash
    ON sessions(refresh_token_hash);
