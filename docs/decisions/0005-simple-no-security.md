# 0005 — A simple branch without security, with automatic joining

## Context

The `main` branch before this change implemented the product as specified: ILMEK/1 over TLS 1.3 with a community CA, enrollment
tokens, signed packages, revocation, and a two-node Wi-Fi routing setup that needs two Wi-Fi
adapters per routing node. In practice this made a first demo hard: nothing could be tried without
creating a CA, issuing tokens, exchanging fingerprints and files, setting up publishers and keys,
and owning the specific radio hardware. The goal for the first version is smaller: one Linux
machine that is the node and also serves applications, one other computer that opens them, with
every machine joining by itself and everything starting without ceremony.

## Decision

Created on the branch `feat/simple-no-security` (approved by the project owner); it is now `main`, and the previous design is kept on the `security` branch:

- **No security in this branch.** Removed: TLS and ALPN, the community CA and certificates,
  enrollment tokens, revocation/trust snapshots, publisher keys and manifest signatures, roles.
  Removed crates: `ilmek-identity`, `ilmek-network`; removed modules: TLS, trust, enrollment.
- **Identity is declared, not proven.** A device creates its own UUIDv7 and states it in `HELLO.client_id`.
  Namespace ownership, deduplication and the principal passed to backends use it.
- **Join by itself.** A node answers UDP probes on port 47400; devices find it by broadcast and
  take the registry address from the answer's source IP. `--registry IP:PORT` replaces discovery.
- **Hosts register by themselves.** A host sends `0.0.0.0:port`; the registry records the address
  the connection came from. A loopback address registered by a host on the registry's machine is
  reached by other devices on the registry's address.
- **No special network.** İlmek requires only IP reachability; the two-Wi-Fi-adapter requirement
  and `ilmek network` are dropped. The node concept stays: the machine with the registry (and
  usually an application host).
- **One command.** `ilmek node` starts registry + host + beacon and creates all state; `ilmek host
  start` adds a host elsewhere; commands that need a device join automatically.
- **Kept:** the frame format, strict JSON, UUIDv7, operation IDs and deduplication, event
  subscriptions with cursors, resumable bulk transfer, SHA-256 integrity checks, the WebView
  content policy and bridge limits, atomic package activation, SQLite persistence and backups.

## Alternatives considered

- Keep TLS but generate certificates automatically: still needs a trust decision (who may join), and
  the user asked to leave security out for now.
- mDNS/Bonjour for discovery: platform differences and a new dependency; a 16-byte UDP probe is
  enough for a single subnet and testable on loopback.
- Keep the old crates and gate security behind a flag: doubles the test matrix and keeps the
  complexity this branch is meant to remove.

## Consequences

- The wire format is **incompatible** with the `security` branch (HELLO field, removed messages, removed record
  fields). A node and a client from different branches cannot talk. `PROTOCOL.md` describes this branch.
- `PROTOCOL.md`, `docs/REQUIREMENTS.md` (product specification of the `security` branch) and the fixtures diverge
  on purpose; `tests/protocol/fixtures/wire_examples.ilmek.json` was regenerated for the new schema,
  the root `WIRE_EXAMPLES.json` was not touched.
- Anyone on the network can read, join, publish and impersonate. See [SECURITY](../SECURITY.md).
- Adding security back is a new protocol version.
