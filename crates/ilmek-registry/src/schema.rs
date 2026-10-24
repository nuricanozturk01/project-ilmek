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

-- The first device that registers an application in a namespace owns it.
-- `publisher_id` is the device id that device declared in HELLO.
CREATE TABLE namespaces (
    namespace TEXT PRIMARY KEY,
    publisher_id TEXT NOT NULL
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
    publisher_id TEXT NOT NULL,
    server_id TEXT NOT NULL REFERENCES hosts(server_id),
    version TEXT NOT NULL,
    manifest_hash TEXT NOT NULL,
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
",
}];
