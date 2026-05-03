pub(crate) const MIGRATION_0001: &str = r#"
CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT OR IGNORE INTO schema_migrations (version) VALUES (1);

CREATE TABLE IF NOT EXISTS admin_account (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    password_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS admin_session (
    id TEXT PRIMARY KEY,
    session_hash TEXT NOT NULL UNIQUE,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS proxy_api_key (
    id TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,
    revoked_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS upstream_secret (
    name TEXT PRIMARY KEY,
    encrypted_value BLOB NOT NULL,
    nonce BLOB NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS request_log (
    id TEXT PRIMARY KEY,
    started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    proxy_key_id TEXT,
    endpoint TEXT NOT NULL,
    requested_model TEXT,
    upstream_model TEXT,
    route_name TEXT,
    routing_match TEXT,
    stream INTEGER NOT NULL DEFAULT 0,
    http_status INTEGER,
    error_category TEXT,
    client_disconnected INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER,
    upstream_first_byte_ms INTEGER,
    time_to_first_token_ms INTEGER,
    generation_ms INTEGER,
    input_tokens INTEGER,
    output_tokens INTEGER,
    total_tokens INTEGER,
    cached_input_tokens INTEGER,
    reasoning_tokens INTEGER,
    accepted_prediction_tokens INTEGER,
    rejected_prediction_tokens INTEGER,
    token_source TEXT,
    provider_usage_json TEXT,
    payload_capture_status TEXT NOT NULL DEFAULT 'not_started',
    payload_capture_error TEXT,
    request_payload_path TEXT,
    response_payload_path TEXT,
    request_payload_bytes INTEGER,
    response_payload_bytes INTEGER,
    request_payload_hash TEXT,
    response_payload_hash TEXT,
    FOREIGN KEY(proxy_key_id) REFERENCES proxy_api_key(id)
);

CREATE INDEX IF NOT EXISTS idx_request_log_started_at ON request_log(started_at);
CREATE INDEX IF NOT EXISTS idx_request_log_proxy_key_id ON request_log(proxy_key_id);
CREATE INDEX IF NOT EXISTS idx_request_log_requested_model ON request_log(requested_model);
CREATE INDEX IF NOT EXISTS idx_request_log_route_name ON request_log(route_name);
"#;
