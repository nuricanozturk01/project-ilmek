# İlmek — Progress Notes (resume file)

Last update: 2026-10-23. Environment: Linux x86_64 dev box (not a NODE/HOST/PHONE). Revision 42167b8 + uncommitted changes (nothing committed).

## State

- fmt, clippy `-D warnings` clean; `cargo test --workspace` 230 passed; JS lint/typecheck/test/build clean.
- Smoke: `scripts/smoke/{local-processes,multi-app,backup-restore}.sh` PASS; `scripts/e2e/isolation-probe.sh` PASS (Linux only).
- Done this session: `ilmek network plan|apply|rollback|status`; `ilmek backup registry|host|verify|restore-*`;
  configs/, deploy/systemd/; docs (README, REQUIREMENTS move, ARCHITECTURE, NETWORK_SETUP, SECURITY,
  APP_DEVELOPMENT, OPERATIONS, IPC, HARDWARE_INVENTORY, TEST_REPORT); P13/P23/renewal tests; test-port flake fix;
  `.deb` built (not installed).
- Decisions: docs/decisions 0001-0004.

## Next executable steps

1. Android client (user will do/test it themselves). Android SDK/NDK/emulator exist on the dev box; no rustup android targets installed.
2. Install and launch the `.deb`; build AppImage (needs network for tooling); Windows/macOS builds.
3. Real hardware: A01 inventory (`ilmek doctor`, `iw list`) then first `ilmek network apply` with console access.
4. Start the systemd units as services on a throwaway machine; adjust OPERATIONS.md.
5. Commit when the user asks.

## Known gaps

Registry CA key is an owner-only file; Windows ACL gap; WebView2/WKWebView/Android isolation unverified;
no physical hardware tested; no CI config.
