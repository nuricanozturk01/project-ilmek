# İlmek — Progress notes (resume file)

Last update: 2026-10-24. Branch `feat/simple-no-security` (from `8da5f7b`), **uncommitted**.
Environment: one Linux x86_64 dev box; nothing else was available.

## State

- `cargo fmt --check`, `cargo clippy --workspace --all-targets` clean; `cargo test --workspace` 178
  passed; JS typecheck/lint/test clean (client 14, sdk 21).
- Desktop client is now a tabbed browser (one webview, sandboxed iframe per app, SDK over `postMessage`); example apps redesigned on a shared `examples/_ui` stylesheet (`scripts/packaging/sync-example-ui.sh`). Checked on Linux only; see TEST_REPORT E1-12.
- Smoke: `scripts/smoke/{local-processes,multi-app,backup-restore}.sh` PASS; `scripts/quickstart.sh`
  works; `crates/ilmek-cli/tests/node_flow.rs` (real binary, UDP discovery, extra host) PASS.
- Done in this branch: security removed (TLS, CA, tokens, trust, signatures); `ilmek-identity` and
  `ilmek-network` crates removed; `ilmek node`, `ilmek host start`, `ilmek join`; UDP discovery
  (`ilmek-core::discovery`); host address filled by the registry; client `host_address`; automatic join
  in the CLI and in the desktop client; `BEGIN IMMEDIATE` for write transactions; docs rewritten
  (README, ARCHITECTURE, NETWORK_AND_DISCOVERY, OPERATIONS, SECURITY, TEST_REPORT, OVERVIEW_TR, PROTOCOL.md,
  APP_DEVELOPMENT_GUIDE, IPC, decision 0005).
- Joining no longer depends on UDP: given an address it uses TCP only (the community id comes from the
  registry's refusal of a `HELLO` for a made-up community), and without an address it runs the UDP beacon
  and a TCP scan of the local /24 in parallel (`ilmek-core::discovery`). Nodes built before this change do
  not return that detail and must be restarted.
- `ilmek://` links: the desktop client registers as the scheme handler (Tauri `deep-link` +
  `single-instance` plugins, `apps/client/src-tauri/src/lib.rs`); a link from a browser or `xdg-open`
  opens the app in the running client. `Cargo.lock` pins `rust-ini` 0.21.1 because
  `tauri-plugin-deep-link` 2.6.2 does not compile against 0.21.3 (Linux-only dependency); remove the pin
  when the plugin is updated.
- Archived (banner added, not updated): `docs/REQUIREMENTS.md`, `DOCUMENT_CHECKS.md`,
  `açıklama/01-03` (describe `main`).

## Next executable steps

1. Try the demo on two real computers (Linux node, Mac client): `ilmek node`, `ilmek join`. Report
   whether UDP broadcast discovery works on that network; fix `discover_on` if the OS needs
   per-interface broadcast addresses.
2. Build the desktop client as a `.app` on the Mac (`cd apps/client && bun run tauri build --bundles app`), open it, join, then `open ilmek://nuri/forum` and Chrome (docs/TWO_MACHINE_DEMO.md §6). Reports are needed: macOS URL-scheme registration is untested.
3. Run `scripts/e2e/isolation-probe.sh` on a machine with a display (not re-run on this branch).
4. Start `ilmek-node.service` on a throwaway machine and correct BACKUP_AND_OPERATIONS.md.
5. Decide when and how to add security back (new protocol version; see SECURITY.md).
6. Commit when the owner asks; decide whether `main` keeps the secure design alongside.

## Known gaps

No security; discovery stays inside one subnet; IPv4 only; nothing tested on a second computer, on
macOS, Windows or Android; the desktop window and `ilmek://` links were exercised on Linux only (not on macOS, not with Chrome); no CI configuration.
