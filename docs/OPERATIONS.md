# Operations

Running the registry and hosts as services, where their data lives, backing up and restoring, and
diagnosing problems.

> **What has and has not been run.** Every `ilmek` command below was run as a normal user in the
> loopback scripts. The systemd units in `deploy/systemd/` pass `systemd-analyze verify` but have
> **never been started as services**, so the installation procedure for them is *not executed*
> and may need adjustment on a real machine. Backup and restore (A22) *was* executed end to end
> against live processes.

## Layout

| Item | Path (suggested; all configurable) |
| --- | --- |
| Binary | `/usr/local/bin/ilmek` |
| Configuration | `/etc/ilmek/registry.toml`, `/etc/ilmek/host.toml` (examples in `configs/`) |
| Registry data | `/var/lib/ilmek/registry`: `registry.db`, `ca.crt.der`, `ca.key.der`, `registry.crt.der`, `registry.key.der`, `community.json` |
| Host data | `/var/lib/ilmek/host`: `host.db`, `apps/<app_id>/versions/<version>/` (immutable), `apps/<app_id>/data/` (application data, `app.db`), `apps/<app_id>/logs/`, `apps/<app_id>/backups/`, `staging/` |
| Host device identity | `/var/lib/ilmek/host-device` (private key, owner-only) |
| Network snapshots | `/var/lib/ilmek/network/<node>-<unix time>/` (root only) |
| Service user | `ilmek` (no login; registry and host run as it, **never as root**) |

The registry and host data directories must be on a persistent local disk. The registry is the
single authority of the community; there is no replica.

## Install a registry or host as a service (not executed)

```sh
cargo build --release -p ilmek-cli
sudo install -m 755 target/release/ilmek /usr/local/bin/ilmek
sudo install -m 644 deploy/systemd/ilmek-sysusers.conf  /usr/lib/sysusers.d/ilmek.conf
sudo install -m 644 deploy/systemd/ilmek-tmpfiles.conf  /usr/lib/tmpfiles.d/ilmek.conf
sudo systemd-sysusers && sudo systemd-tmpfiles --create
sudo install -m 644 deploy/systemd/ilmek-registry.service /etc/systemd/system/    # registry machine
sudo install -m 644 deploy/systemd/ilmek-host.service     /etc/systemd/system/    # host machine
```

**Registry machine.** Copy `configs/registry.toml.example` to `/etc/ilmek/registry.toml` and set
the listen addresses and `allowed_endpoints` (networks hosts may advertise). Create the community
as the service user, write down the printed CA fingerprint, then start:

```sh
sudo -u ilmek ilmek community init --config /etc/ilmek/registry.toml --advertise-ip 10.77.10.1
sudo systemctl daemon-reload && sudo systemctl enable --now ilmek-registry
```

`--advertise-ip` becomes the registry certificate's IP SAN; clients refuse the registry at any
other address. Hand `community.json` and the fingerprint to new devices (the fingerprint on a
separate channel from the file is better).

**Host machine.** Copy `configs/host.toml.example` to `/etc/ilmek/host.toml`; `advertised` must be
the address clients can reach (the host refuses an unspecified address). The registry
administrator issues a host token for this machine's IP, and the host enrolls and starts:

```sh
# on the registry machine
sudo -u ilmek ilmek community invite --config /etc/ilmek/registry.toml --role member,host --server-ip 10.77.10.10 > host.token
# on the host machine (copy host.token, community.json and the fingerprint over)
sudo -u ilmek ilmek identity enroll --profile community.json --fingerprint <64 hex> \
     --device /var/lib/ilmek/host-device --name host-1 --token-file host.token
sudo systemctl enable --now ilmek-host
```

Applications are then installed and published with `ilmek app …` ([APP_DEVELOPMENT](APP_DEVELOPMENT.md)).
Backend executables must be readable and executable by the `ilmek` user and outside `/home`
(the unit sets `ProtectHome=yes`).

## Certificates expire after 30 days

Devices and hosts: 30 days from enrollment. The registry's own certificate: 30 days unless
`community init --identity-days` set another value. There is no automatic renewal. Put a reminder
in your calendar, or you will find out when everything fails with `CertificateExpired`.

```sh
# registry certificate (run as the service user, then restart)
sudo -u ilmek ilmek community reissue-identity --config /etc/ilmek/registry.toml --advertise-ip 10.77.10.1
sudo systemctl restart ilmek-registry

# a device or host: the administrator issues a renewal token bound to the existing identity
ilmek community invite --config registry.toml --role member,host --server-ip 10.77.10.10 --bound-peer <peer id> > renew.token
ilmek identity enroll --profile community.json --fingerprint <hex> --device <new dir> --token-file renew.token
```

Renewal keeps the peer ID, so names and grants stay. The enrollment endpoint needs no client
certificate, so a bound token should also recover a device whose certificate has already expired
(not tested with a truly expired certificate). A revoked or unknown peer cannot renew.
Covered by software tests (`crates/ilmek-registry/tests/renewal.rs`); waiting out a real 30 days
has not been done.

## Backup and restore

SQLite files are copied with SQLite's online backup API, so the services keep running and the copy
is consistent (WAL included); everything else is copied byte for byte; every file is hashed in a
manifest; logs, earlier backups and staging areas are excluded.

```sh
ilmek backup registry --config /etc/ilmek/registry.toml --out /backup/registry-$(date +%F) [--include-keys]
ilmek backup host     --config /etc/ilmek/host.toml     --out /backup/host-$(date +%F)
ilmek backup verify   --dir /backup/host-2026-10-23         # reads only; also checks SQLite integrity
```

- The output directory must not exist (nothing is ever overwritten); it is assembled aside and
  renamed into place, so an interrupted run leaves no finished-looking backup.
- Private keys (`*.key.der`) are **not** copied unless you pass `--include-keys`. Without them a
  restored registry cannot issue certificates or serve; with them the backup is as sensitive as the
  CA itself: keep it encrypted and apart from ordinary log archives. The host device identity
  (`host-device/`) is not part of a host backup; re-enroll the host instead, or copy that directory
  separately like a key.
- Each file is consistent; the set of files is not a single atomic snapshot. For a host with busy
  applications, stop the host first if you need all applications at exactly the same instant.

Restore into a **new or empty** data directory (the command refuses anything else):

```sh
sudo systemctl stop ilmek-host
sudo mv /var/lib/ilmek/host /var/lib/ilmek/host.broken
sudo -u ilmek ilmek backup restore-host --config /etc/ilmek/host.toml --from /backup/host-2026-10-23
sudo systemctl start ilmek-host
# registry: the same with `restore-registry`; copy the key files back first if the backup had none
```

The backup is verified (hashes, SQLite integrity, safe paths) before anything is written; data is
assembled in a sibling directory and renamed, so a failed restore leaves the target untouched.
Exercise: `scripts/smoke/backup-restore.sh` backs up a live registry and a host running the forum
with data, creates more data, destroys both data directories, restores, restarts, and checks that
the state equals the backup point and the application accepts writes (result in
[TEST_REPORT](TEST_REPORT.md), A22). Restore a backup at least once before you depend on it.

## Upgrading

Take a backup first. Databases migrate forward only at start-up, and a database written by a newer
build is refused by an older one — so going back to an old binary after a migration means
restoring the pre-upgrade backup. Replace `/usr/local/bin/ilmek`, restart the service. Backends are
separate executables; the host copies `app.db` aside when it starts a backend whose version
differs from the last one it started.

## Diagnosing

```sh
ilmek doctor                                        # read-only: OS, CPU/RAM, interfaces, routes, iw, rfkill, USB/PCI
ilmek diagnose ilmek://nuri/forum --device <device> # trust snapshot → resolve → host reachable → package
ilmek resolve  ilmek://nuri/forum --device <device> # what the registry says, and where it came from
ilmek app list --config /etc/ilmek/host.toml        # state, pid, restarts per application
journalctl -u ilmek-registry -u ilmek-host -f
```

Host and backend logs for an application are in `apps/<app_id>/logs/` (rotated at 1 MiB, 3 files
kept). Logs carry request IDs, error classes, durations and sizes, not tokens, keys or bodies.

| Symptom | Likely cause | What to do |
| --- | --- | --- |
| `CertificateExpired` / TLS alert on every connection | a 30-day certificate ran out (device, host or registry) | renew as above |
| `TRUST_STALE` | the device or host could not reach the registry for more than 24 h | restore connectivity to the registry; it refreshes automatically |
| `diagnose`: name resolves, `Availability: Stale` | the host's lease lapsed (host down, or cannot reach the registry) | start the host / check its route to the registry |
| `APP_UNAVAILABLE` | that application's backend is stopped, crashing or abandoned | `app list`; `app restart`; read its log |
| `FORBIDDEN` after it worked | the device was revoked, or the publisher key/grant removed | `community peers`; re-enroll with a new token |
| enrollment refused | token used, expired (15 min default) or wrong CA fingerprint | issue a new token; check the fingerprint |
| host refuses to start: advertised address | `advertised` is unspecified or not IPv4 | set the real address |
| install refused | publisher key not trusted on this host, version not newer, or hash/signature mismatch | `host trust-publisher`; fix the package |
| `RESOURCE_EXHAUSTED` | per-application queue or in-flight limit, or connection limit | tune `app_max_inflight`, `app_queue_depth`, `max_connections` |

With the registry down, already-resolved names keep working from the client's cache and existing
hosts stay reachable until the 24 h trust window ends; new name resolution and publishing do not
work (REQUIREMENTS §12 and tests `names_resolve_from_the_cache_while_the_registry_is_down…`).
