# Architecture

Normative references: [PROTOCOL.md](../PROTOCOL.md) (wire format and behavior) and
[REQUIREMENTS.md](REQUIREMENTS.md) (product scope). This file describes how the code in this
repository is organized and where the trust boundaries are. It does not restate the protocol.

## Roles

| Role | Runs | Persistent state |
| --- | --- | --- |
| **Registry** (`ilmek registry serve`) | the community CA, enrollment, name ownership, host leases, trust snapshots (revocations) | `registry.db`, CA certificate and key, registry identity, `community.json` |
| **Host** (`ilmek host serve`) | any number of applications, each with its own `app_id`, data directory and lifecycle | `host.db`, `apps/<app_id>/{versions,data,logs}` |
| **Publisher** | a person holding a signing key, using the CLI | publisher key file (owner-only) |
| **Client** (desktop app, later Android) | resolves, downloads, verifies and runs applications | device identity, package cache, per-app local storage |
| **Routing node** | Linux machine with two radios: user access point plus transit link | `/etc/ilmek`, systemd units, a rollback snapshot |

The registry is **not** a proxy. It answers "who serves `ilmek://nuri/forum`, which version and
hash"; the client then connects to the host directly. A host does not have to be a routing node,
and applications never run on a node unless you put a host there.

```
 client ──resolve (mTLS, 47401)──▶ registry
   │                                  ▲
   │ connect (mTLS, 47401)            │ register / lease heartbeat (host, publisher)
   ▼                                  │
  host ──IPC (loopback, token)──▶ application backend (separate process)
```

Ports: control and application traffic use TCP 47401; the registry's enrollment endpoint is TCP
47402 (server-authenticated TLS plus a single-use token). All are configurable.

## Layers

`Wi-Fi → IP → TCP → TLS 1.3 → ILMEK/1`. Wi-Fi, IP and TCP come from the operating system;
`ilmek-network` only *configures* them with ordinary Linux tools (`hostapd`, `wpa_supplicant`,
`dnsmasq`, `nftables`, `ip`). The protocol never tunnels IP packets.

## Crates

```
ilmek-protocol  frames, strict JSON, ids, manifest types          (pure, no I/O)
ilmek-core      TLS identity checks, framed transport, sessions   (tokio, rustls)
ilmek-identity  CA, device keys/CSRs, publisher keys, profile
ilmek-package   pack / sign / verify / copy packages
ilmek-store     SQLite: migrations, online backup, tree backup/restore
ilmek-registry  enrollment, names, leases, trust snapshots
ilmek-host      installer, supervisor, ILMEK/1 service, IPC server
ilmek-app-sdk   backend side of the IPC (Rust)
ilmek-client    enroll, connect, resolve, download, call
ilmek-runtime   page bridge: permissions, storage, subscriptions
ilmek-network   node plan / apply / rollback
ilmek-cli       the `ilmek` binary
ilmek-testing   fixtures for integration tests (never a production dependency)
```

Dependency direction is one way: `protocol` ← `core` ← (`identity`, `client`, `registry`, `host`) ←
`cli`. `ilmek-network` and `ilmek-store` have no protocol dependency.

## Main flows

**Enrollment.** The administrator runs `community init` (creates the CA, the registry certificate
and `community.json`) and prints the CA fingerprint. A new device receives `community.json`, the
fingerprint and a single-use token out of band, generates its key and CSR locally, and enrolls on
the enrollment endpoint. The CA fingerprint is mandatory; the profile alone is never trusted. The
CA chooses the identity in the certificate; names requested in the CSR are ignored.

**Publishing.** The publisher packs a project (`app pack`) into a directory with a sorted,
hashed `manifest.json` and a detached signature. The host verifies it against publisher keys the
administrator trusts (`host trust-publisher`) and activates it atomically. `app publish` registers
name, version, manifest hash and endpoint in the registry; the host renews its lease by heartbeat.

**Resolving and fetching.** The client asks the registry for the record, connects to the host on a
*control* connection for the manifest and on a separate *bulk* connection for files. It checks the
manifest signature against the key in the registry record, every file's size and SHA-256, and only
then moves the package into place. Interrupted downloads resume from verified bytes.

**Calling.** The page calls `sdk.call(method, input)`; the runtime checks the manifest permission,
adds the verified application identity, and sends `APP_CALL` over mTLS. The host derives the
caller from the certificate (never from the request), checks the application and method, and
forwards to the backend over loopback IPC. Writes carry an operation ID; the backend commits the
business change and its deduplication record in one SQLite transaction, so a retry after a lost
reply returns the original result.

**Revocation.** The registry publishes a paged, versioned trust snapshot. Hosts and clients refresh
it (default every 60 s). A snapshot takes effect only after its last page; work is refused when
the local snapshot is older than 24 hours. Revoking a peer ends its idle sessions and rejects its
next request.

## Process and trust boundaries

1. **Network ↔ host/registry:** mutual TLS 1.3, ALPN `ilmek/1`, certificate chain to the community
   CA, the server's IP SAN and peer ID checked by the client. Sessions are never resumed.
2. **Host ↔ backend:** loopback only, a per-process token, an application ID bound at start-up.
   The backend is a **trusted** local process; being a separate process is not a sandbox.
3. **Page ↔ runtime:** the application's HTML/JS is **untrusted**. It runs in a platform WebView
   with a strict content policy, and its only channel is the SDK bridge, which has no field for a
   host, address or file path. See [SECURITY](SECURITY.md).
4. **Operator ↔ privileged setup:** Wi-Fi/IP/firewall changes are separate from the application
   runtime, need root, and are always reversible through a snapshot. The registry and host run as
   an unprivileged user.

## Persistence

SQLite with forward-only migrations and a schema-version check (a database from a newer build is
refused). Registry and host write business data and deduplication results in one transaction.
Backups use SQLite's online backup API ([OPERATIONS](OPERATIONS.md)). Package versions are
immutable directories; user data lives beside them and is kept on removal unless `--purge`.

## What is deliberately not built (V1)

Bluetooth, mesh/WDS/VLAN, store-and-forward between network segments, CRDT or database merging,
registry high availability, NAT or any route to the public internet. See REQUIREMENTS §3 and §21.
