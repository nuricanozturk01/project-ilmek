# Operations

Running a node and extra hosts as services, where their data lives, backing up and restoring, and
diagnosing problems.

> **What has and has not been run.** Every `ilmek` command below was run as a normal user in the
> loopback scripts and tests (`scripts/smoke/*`, `crates/ilmek-cli/tests/node_flow.rs`). The
> systemd units in `deploy/systemd/` were **never started as services and never checked with
> `systemd-analyze`**, so the installation procedure for them is *not executed* and may need
> adjustment on a real machine. Backup and restore *was* executed end to end against live
> processes.

## Layout

Everything lives under one directory, `$ILMEK_HOME` (default `~/.ilmek`; `/var/lib/ilmek` for the
service).

| Item | Path |
| --- | --- |
| Binary | `/usr/local/bin/ilmek` (your choice) |
| Node configuration | `<home>/registry.toml`, `<home>/host.toml`; written on the first `ilmek node`, edit them afterwards |
| Registry data | `<home>/registry/`: `registry.db`, `community.json` |
| Host data | `<home>/host/`: `host.db`, `apps/<app_id>/versions/<version>/` (immutable), `apps/<app_id>/data/` (application data, `app.db`), `apps/<app_id>/logs/`, `apps/<app_id>/backups/`, `staging/` |
| Device state (this machine as a client or publisher) | `<home>/device/`: `profile.json`, `device.id`, name cache |
| Host's own device state | `<home>/host-device/` |
| Package cache of `ilmek fetch` | `<home>/cache/` |

The data directories must be on a persistent local disk. The registry is the single authority of
the community; there is no replica.

## Run a node

```sh
ilmek node                    # foreground; Ctrl-C shuts down in order
ilmek node --local-only       # listens on 127.0.0.1 only (no other device can connect)
ilmek node --registry-port 5000 --host-port 5001 --discovery-port 5002
```

The first run creates the community and the configuration files; the port flags only apply when
those files are written. Afterwards `registry.toml` / `host.toml` decide, and the node announces the
port it actually listens on, so editing a port there and restarting is enough (a test covers it).

### As a service (not executed)

```sh
cargo build --release -p ilmek-cli
sudo install -m 755 target/release/ilmek /usr/local/bin/ilmek
sudo install -m 644 deploy/systemd/ilmek-sysusers.conf /usr/lib/sysusers.d/ilmek.conf
sudo install -m 644 deploy/systemd/ilmek-tmpfiles.conf /usr/lib/tmpfiles.d/ilmek.conf
sudo systemd-sysusers && sudo systemd-tmpfiles --create
sudo install -m 644 deploy/systemd/ilmek-node.service /etc/systemd/system/   # the node
sudo systemctl daemon-reload && sudo systemctl enable --now ilmek-node
```

On an additional machine use `ilmek-host.service` (`ilmek host start`) instead. Backend
executables must be readable and executable by the `ilmek` user and outside `/home` (the unit sets
`ProtectHome=yes`). Both units set `ILMEK_HOME=/var/lib/ilmek`; run the `ilmek app …` commands with
the same variable (`sudo -u ilmek env ILMEK_HOME=/var/lib/ilmek ilmek app list`).

## Backup and restore

SQLite files are copied with SQLite's online backup API, so the services keep running and the copy
is consistent (WAL included); everything else is copied byte for byte; every file is hashed in a
manifest; logs, earlier backups and staging areas are excluded.

```sh
ilmek backup registry --out /backup/registry-$(date +%F)
ilmek backup host     --out /backup/host-$(date +%F)
ilmek backup verify   --dir /backup/host-2026-10-23         # reads only; also checks SQLite integrity
```

(`--config` selects another `registry.toml` / `host.toml`; the default is the file in `<home>`.)

- The output directory must not exist (nothing is ever overwritten); it is assembled aside and
  renamed into place, so an interrupted run leaves no finished-looking backup.
- Each file is consistent; the set of files is not a single atomic snapshot. For a host with busy
  applications, stop it first if you need all applications at exactly the same instant.
- The device state directories (`device/`, `host-device/`) are not part of a backup. They only
  hold the profile and a random id; a lost one is replaced by joining again, but the node then sees
  a new device (a host registers as a new host; namespaces stay with the old id).

Restore into a **new or empty** data directory (the command refuses anything else):

```sh
mv ~/.ilmek/host ~/.ilmek/host.broken
ilmek backup restore-host --from /backup/host-2026-10-23
```

The backup is verified (hashes, SQLite integrity, safe paths) before anything is written; data is
assembled in a sibling directory and renamed, so a failed restore leaves the target untouched.
`scripts/smoke/backup-restore.sh` backs up a live registry and a host running the forum with data,
creates more data, destroys both data directories, restores, restarts, and checks that the state
equals the backup point and the application accepts writes (result in [TEST_REPORT](TEST_REPORT.md)).
Restore a backup at least once before you depend on it.

## Upgrading

Take a backup first. Databases migrate forward only at start-up, and a database written by a newer
build is refused by an older one. Going back to an old binary after a migration means restoring the
pre-upgrade backup. Replace the binary and restart. This branch's registry/host schemas and wire
format are **not compatible with the `security` branch**: do not point it at data created there.

## Diagnosing

```sh
ilmek whoami                                  # this device's id and registry
ilmek diagnose ilmek://nuri/forum             # device → registry → name → host → package
ilmek resolve  ilmek://nuri/forum             # what the registry says
ilmek app list                                # state, pid, restarts per application (on the host)
```

Host and backend logs for an application are in `apps/<app_id>/logs/` (rotated at 1 MiB, 3 files
kept). Logs carry request IDs, error classes, durations and sizes, not bodies.

| Symptom | Likely cause | What to do |
| --- | --- | --- |
| `no node found` on `ilmek join` | different subnet, node not running, firewall dropping both UDP 47400 and TCP 47401 | open `47401-47402/tcp` (and `47400/udp`) on the node; or `ilmek join --registry IP:47401` |
| `… did not answer like an İlmek registry` | another program on that port, or the node was started from an older build | rebuild and restart the node; devices and node must run the same build |
| `TCP connection failed` / timed out | wrong address, node down, firewall | check `ilmek node` is running; test `nc -vz IP 47401` |
| `diagnose`: name resolves, `Availability: Stale` | the host's lease lapsed (host down, or cannot reach the registry) | start the host / check its route to the node |
| `APP_UNAVAILABLE` | that application's backend is stopped, crashing or abandoned | `app list`; `app restart`; read its log |
| `FORBIDDEN` on publish | the namespace or app was first registered by another device id | publish from that device, or pick another namespace |
| `publishing failed: … host is not registered` | the host has not yet registered with the registry | wait a few seconds (the command retries for 30 s) |
| `RESOURCE_EXHAUSTED` | per-application queue or in-flight limit, or connection limit | tune `app_max_inflight`, `app_queue_depth`, `max_connections` |
| `database is locked` | should not occur (write transactions are `IMMEDIATE`); report it with the command that caused it | — |

With the registry down, already-resolved names keep working from the client's cache for up to 24
hours; new name resolution and publishing do not work.
