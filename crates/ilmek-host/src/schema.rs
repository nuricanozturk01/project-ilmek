use ilmek_store::Migration;

/// Host schema. Forward-only; v1 is the initial layout.
pub(crate) const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "initial",
    sql: "
CREATE TABLE apps (
    app_id TEXT PRIMARY KEY,
    namespace TEXT NOT NULL,
    slug TEXT NOT NULL,
    publisher_id TEXT NOT NULL,
    name TEXT NOT NULL,
    backend_required INTEGER NOT NULL,
    active_version TEXT NOT NULL,
    desired TEXT NOT NULL DEFAULT 'stopped' CHECK (desired IN ('running', 'stopped')),
    restart_generation INTEGER NOT NULL DEFAULT 0,
    created_at_ms INTEGER NOT NULL,
    UNIQUE (namespace, slug)
) STRICT;

CREATE TABLE versions (
    app_id TEXT NOT NULL REFERENCES apps(app_id),
    version TEXT NOT NULL,
    manifest_hash TEXT NOT NULL,
    directory TEXT NOT NULL,
    installed_at_ms INTEGER NOT NULL,
    PRIMARY KEY (app_id, version)
) STRICT;

CREATE TABLE backends (
    app_id TEXT PRIMARY KEY REFERENCES apps(app_id),
    executable TEXT NOT NULL,
    args TEXT NOT NULL,
    accepted_versions TEXT NOT NULL
) STRICT;

CREATE TABLE status (
    app_id TEXT PRIMARY KEY REFERENCES apps(app_id),
    state TEXT NOT NULL,
    pid INTEGER,
    restarts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    updated_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE registry_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    host_revision INTEGER NOT NULL
) STRICT;
INSERT INTO registry_state (id, host_revision) VALUES (1, 0);
",
}];
