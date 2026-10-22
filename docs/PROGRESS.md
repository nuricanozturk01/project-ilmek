# Progress

Environment: Linux x86_64, 16 cores, rustc/cargo 1.96.0, node 24.17, pnpm 12.14, sqlite 3.50,
adb 1.0.41, docker 29.9. Missing: `just`, `iw`. No git repository yet. This host is a dev machine,
not one of the NODE/HOST/PHONE devices; no network configuration has been touched.

## Done

- Stage 0: workspace (`Cargo.toml`, toolchain pin, lints, rustfmt/clippy config).
- Stage 1 (partial): `crates/ilmek-protocol` — frame codec, strict JSON, UUIDv7, message catalog,
  envelope, hash preimages. No sockets or TLS yet.
- `crates/ilmek-identity`: community CA, CSR-based leaf issuance (CA decides SANs, validity, key
  usage; requester fields ignored), peer-id extraction, Ed25519 publisher keys/manifest signatures
  (fixture signature verifies and is reproduced from the test seed).
- `crates/ilmek-core::tls`: TLS 1.3-only rustls (ring) client/server, ALPN `ilmek/1`, resumption
  and early data off, mTLS + enrollment endpoint, post-handshake peer-id check, handshake deadlines.
- Fixtures: supplied file carried `ADA` identifiers; regenerated for ILMEK, see
  `docs/decisions/0001-fixture-rename.md`.

## Evidence

`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`: clean, 45 tests passing (2026-10-22, linux x86_64, no VCS revision).

## Conformance (PROTOCOL.md §20)

| ID | Status | Note |
| --- | --- | --- |
| P01 | PASS | fixtures round-trip byte-for-byte |
| P02 | PASS | byte-at-a-time + proptest chunking |
| P03 | PASS | two frames in one read |
| P04 | PASS | lengths rejected from the 12-byte prefix, before payload buffering |
| P05 | PASS | UUIDv7 rules, duplicate keys, BOM, bad UTF-8 (unknown-field rejection of typed `data` schemas still open) |
| P06 | PASS (loopback) | wrong CA (both directions), wrong IP SAN, wrong peer id, expired cert, foreign ALPN, non-TLS bytes; client-side ALPN check untested |
| P07–P24 | NOT RUN | need HELLO state machine, registry, host |

A01–A24: all NOT RUN.

## Next

1. Revocation: trust snapshot store + serial check after handshake (P17, P18, P22, A24).
2. Typed `data` schemas per message (strict, unknown fields rejected).
3. `ilmek-core` connection: frame IO over TLS, HELLO/HELLO_ACK state machine (P07), deadlines
   (30 s frame, 5 s HELLO), single writer queue.
4. Two-process test: HELLO + GET_MANIFEST + bulk asset with hash check (stage 1 gate).
5. CA/key persistence (`ilmek community init`), key file permissions.
6. Replace stale spec claims about fixtures (owner of README/PROTOCOL).

## Known gaps

- Leaf serial numbers are rcgen-derived (from the public key), not random; revisit with revocation.
- No Wi-Fi, Android, or multi-host evidence of any kind.
