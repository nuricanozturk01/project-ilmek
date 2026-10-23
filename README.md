# İlmek

İlmek is an internet-independent community application network: devices on a local Wi-Fi network
resolve an `ilmek://namespace/app` address through a registry, connect straight to the host that
serves the application, verify its signed package, and run its HTML/CSS/JavaScript in an isolated
native runtime. Everything runs over `Wi-Fi → IP → TCP → TLS 1.3 → ILMEK/1`; there is no HTTP,
WebSocket or cloud fallback.

> **Status, stated plainly.** The software is implemented and tested on one Linux x86_64 machine,
> with separate OS processes talking over loopback. **No physical Wi-Fi, two-node routing, Android,
> Windows or macOS check has been run.** Those items are marked `BLOCKED` or `NOT RUN` in
> [docs/TEST_REPORT.md](docs/TEST_REPORT.md), which is the authority on what has and has not been
> verified. Nothing here is claimed production-ready.

The original product specification is preserved unchanged in
[docs/REQUIREMENTS.md](docs/REQUIREMENTS.md); the wire contract is [PROTOCOL.md](PROTOCOL.md).
Where this file and those differ, they win.

## What is in the repository

| Path | What it is |
| --- | --- |
| `crates/ilmek-protocol` | ILMEK/1 frames, strict metadata, identifiers, manifests (fixtures from `WIRE_EXAMPLES.json` are tested) |
| `crates/ilmek-core` | TLS 1.3 identity checks, framed transport, sessions, trust snapshots |
| `crates/ilmek-identity` | community CA, device certificates, publisher signing keys |
| `crates/ilmek-registry` | enrollment, names, trust snapshots, host leases, persistence |
| `crates/ilmek-host` | multi-application host: packages, backend supervision, the ILMEK/1 service |
| `crates/ilmek-client`, `crates/ilmek-runtime` | resolve/download/call, and the page bridge with local storage |
| `crates/ilmek-package`, `crates/ilmek-store` | signed packages; SQLite with migrations and backups |
| `crates/ilmek-app-sdk` | Rust SDK for application backends |
| `crates/ilmek-network` | plan / apply / roll back the Wi-Fi, IP, DHCP and firewall setup of a routing node |
| `crates/ilmek-cli` | the `ilmek` command |
| `packages/sdk-js` | `@ilmek/sdk`, interface `IlmekAppSDK` |
| `apps/client` | desktop client (Tauri shell; React + TypeScript UI) |
| `examples/` | forum and notes (with backends), announcements and hello (static), isolation probe |
| `configs/` | example `node-a`, `node-b`, `registry`, `host` configuration |
| `deploy/systemd/` | service units for registry and host, user and directory definitions |
| `scripts/` | loopback multi-process smoke tests, backup/restore exercise, isolation probe |

Documentation: [ARCHITECTURE](docs/ARCHITECTURE.md) · [NETWORK_SETUP](docs/NETWORK_SETUP.md) ·
[SECURITY](docs/SECURITY.md) · [APP_DEVELOPMENT](docs/APP_DEVELOPMENT.md) ·
[OPERATIONS](docs/OPERATIONS.md) · [IPC](docs/IPC.md) ·
[HARDWARE_INVENTORY](docs/HARDWARE_INVENTORY.md) · [TEST_REPORT](docs/TEST_REPORT.md) ·
[decisions](docs/decisions) · [PROGRESS](docs/PROGRESS.md)

## Build and test

Toolchain versions are pinned in `rust-toolchain.toml` and `package.json`. Verified on Linux x86_64
(Rust 1.96.0, Node 24, pnpm 12).

```sh
cargo build -p ilmek-cli                       # target/debug/ilmek
cargo test --workspace                         # Rust tests
cargo clippy --workspace --all-targets -- -D warnings
pnpm install && pnpm -r lint && pnpm -r typecheck && pnpm -r test && pnpm -r build
```

## Try it on one machine (loopback only)

This runs a registry, a host and several devices as **separate processes on `127.0.0.1`**. It
exercises the real protocol and persistence; it is not a Wi-Fi or multi-machine test.

```sh
scripts/smoke/local-processes.sh     # publish → resolve → download → call → restart → revoke
scripts/smoke/backup-restore.sh      # back up live registry+host, destroy data, restore, verify
```

The steps those scripts perform, in order (each command was run by them):

```sh
ilmek community init   --config registry.toml --advertise-ip 127.0.0.1   # prints the CA fingerprint
ilmek registry serve   --config registry.toml
ilmek community invite --config registry.toml --role member > alice.token    # single use, 15 min
ilmek identity enroll  --profile registry-data/community.json --fingerprint <fingerprint> \
                       --device alice-device --name alice --token-file alice.token
ilmek app pack  ./my-app --key publisher.key --out ./my-app-pkg --publisher-id <publisher-id>
ilmek app install --config host.toml ./my-app-pkg
ilmek app start   --config host.toml <namespace>/<slug>
ilmek app publish --config host.toml <namespace>/<slug> --device publisher-device
ilmek resolve  ilmek://<namespace>/<slug> --device alice-device
ilmek diagnose ilmek://<namespace>/<slug> --device alice-device
```

The registry and publisher setup (`add-publisher`, `grant-namespace`, `grant-host`,
`host trust-publisher`) is shown end to end in `scripts/lib/community.sh`.
[APP_DEVELOPMENT](docs/APP_DEVELOPMENT.md) explains how to write and publish an application.

## The two-node network

[NETWORK_SETUP](docs/NETWORK_SETUP.md) describes the NODE-A / NODE-B setup and the
`ilmek network plan | apply | rollback | status` commands. `plan` only reads the machine.
`apply` needs root, prints the plan first, refuses without `--yes`, refuses to touch the interface
carrying your default route or login, and records a snapshot so `rollback` can undo it.
**It has not been applied to a real node.**

## Limits to know before relying on it

- Not tested on real Wi-Fi, on Android, Windows or macOS; WebView isolation is verified for Linux
  (WebKitGTK) only.
- Device and registry certificates last 30 days; renewal is implemented but has only been
  exercised in software tests ([OPERATIONS](docs/OPERATIONS.md)).
- Application backends are trusted local processes, not a security sandbox.
- The registry is a single authoritative writer; there is no failover.
- Windows file-permission (ACL) hardening of key files is not implemented.
