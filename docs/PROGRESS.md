# İlmek — Progress notes (resume file)

Last update: 2026-10-24. Branch `feat/simple-no-security` (from `8da5f7b`), **uncommitted**.
Environment: one Linux x86_64 dev box; nothing else was available.

## State

- `cargo fmt --check`, `cargo clippy --workspace --all-targets` clean; `cargo test --workspace` 172
  passed; JS typecheck/lint/test clean (client 13, sdk 20).
- Smoke: `scripts/smoke/{local-processes,multi-app,backup-restore}.sh` PASS; `scripts/quickstart.sh`
  works; `crates/ilmek-cli/tests/node_flow.rs` (real binary, UDP discovery, extra host) PASS.
- Done in this branch: security removed (TLS, CA, tokens, trust, signatures); `ilmek-identity` and
  `ilmek-network` crates removed; `ilmek node`, `ilmek host start`, `ilmek join`; UDP discovery
  (`ilmek-core::discovery`); host address filled by the registry; client `host_address`; automatic join
  in the CLI and in the desktop client; `BEGIN IMMEDIATE` for write transactions; docs rewritten
  (README, ARCHITECTURE, NETWORK_SETUP, OPERATIONS, SECURITY, TEST_REPORT, REHBER, PROTOCOL.md,
  APP_DEVELOPMENT, IPC, decision 0005).
- Archived (banner added, not updated): `docs/REQUIREMENTS.md`, `DOCUMENT_CHECKS.md`,
  `açıklama/01-03` (describe `main`).

## Next executable steps

1. Try the demo on two real computers (Linux node, Mac client): `ilmek node`, `ilmek join`. Report
   whether UDP broadcast discovery works on that network; fix `discover_on` if the OS needs
   per-interface broadcast addresses.
2. Build and run the desktop client on the Mac (`pnpm --filter @ilmek/client tauri dev`).
3. Run `scripts/e2e/isolation-probe.sh` on a machine with a display (not re-run on this branch).
4. Start `ilmek-node.service` on a throwaway machine and correct OPERATIONS.md.
5. Decide when and how to add security back (new protocol version; see SECURITY.md).
6. Commit when the owner asks; decide whether `main` keeps the secure design alongside.

## Known gaps

No security; discovery stays inside one subnet; IPv4 only; nothing tested on a second computer, on
macOS, Windows or Android; the desktop window was not started in this work; no CI configuration.
