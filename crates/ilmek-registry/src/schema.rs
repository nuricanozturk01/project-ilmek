use ilmek_store::Migration;

/// Registry schema. Forward-only; v1 is the initial layout, so rollback means restoring a backup.
pub(crate) const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "initial",
    sql: "
CREATE TABLE community (
    id TEXT PRIMARY KEY,
    registry_id TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE peers (
    peer_id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    roles TEXT NOT NULL,
    cert_serial TEXT NOT NULL,
    cert_not_after_ms INTEGER NOT NULL,
    revoked_at_ms INTEGER,
    created_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE enrollment_tokens (
    token_hash TEXT PRIMARY KEY,
    roles TEXT NOT NULL,
    bound_peer_id TEXT,
    server_ip TEXT,
    expires_at_ms INTEGER NOT NULL,
    consumed_at_ms INTEGER,
    created_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE enrollments (
    operation_id TEXT PRIMARY KEY,
    token_hash TEXT NOT NULL,
    csr_hash TEXT NOT NULL,
    result_json TEXT NOT NULL,
    retain_until_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE publishers (
    publisher_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE publisher_devices (
    publisher_id TEXT NOT NULL REFERENCES publishers(publisher_id),
    peer_id TEXT NOT NULL REFERENCES peers(peer_id),
    PRIMARY KEY (publisher_id, peer_id)
) STRICT;

CREATE TABLE publisher_keys (
    key_id TEXT PRIMARY KEY,
    publisher_id TEXT NOT NULL REFERENCES publishers(publisher_id),
    public_key BLOB NOT NULL,
    revoked_at_ms INTEGER
) STRICT;

CREATE TABLE namespaces (
    namespace TEXT PRIMARY KEY,
    publisher_id TEXT NOT NULL REFERENCES publishers(publisher_id)
) STRICT;

CREATE TABLE publisher_hosts (
    publisher_id TEXT NOT NULL REFERENCES publishers(publisher_id),
    server_id TEXT NOT NULL,
    PRIMARY KEY (publisher_id, server_id)
) STRICT;

CREATE TABLE hosts (
    server_id TEXT PRIMARY KEY,
    ip TEXT NOT NULL,
    port INTEGER NOT NULL,
    revision INTEGER NOT NULL,
    lease_expires_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE apps (
    app_id TEXT PRIMARY KEY,
    namespace TEXT NOT NULL,
    slug TEXT NOT NULL,
    publisher_id TEXT NOT NULL REFERENCES publishers(publisher_id),
    server_id TEXT NOT NULL REFERENCES hosts(server_id),
    version TEXT NOT NULL,
    manifest_hash TEXT NOT NULL,
    publisher_key_id TEXT NOT NULL REFERENCES publisher_keys(key_id),
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    revision INTEGER NOT NULL,
    unregistered INTEGER NOT NULL DEFAULT 0,
    updated_at_ms INTEGER NOT NULL,
    UNIQUE (namespace, slug)
) STRICT;

CREATE TABLE operations (
    principal_id TEXT NOT NULL,
    message_type TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    request_json TEXT NOT NULL,
    result_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (principal_id, message_type, operation_id)
) STRICT;

CREATE TABLE revocations (
    kind TEXT NOT NULL CHECK (kind IN ('certificate', 'publisher_key')),
    value TEXT NOT NULL,
    revoked_at_ms INTEGER NOT NULL,
    PRIMARY KEY (kind, value)
) STRICT;

CREATE TABLE trust_revision (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    revision INTEGER NOT NULL
) STRICT;
INSERT INTO trust_revision (id, revision) VALUES (1, 1);
",
}];
