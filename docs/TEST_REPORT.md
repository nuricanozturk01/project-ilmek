# Test report

**Branch:** `feat/simple-no-security`, based on `8da5f7b`, with **uncommitted** working-tree changes
(nothing has been committed). **Date:** 2026-10-24.
**Environment E1:** one Linux x86_64 development PC (Linux 7.0, rustc 1.96.0, Node 24.17.0, pnpm
12.14.0), loopback networking only. **No other environment was available.** Nothing below ran on a
second computer, on a real LAN, on macOS, Windows or Android.

Statuses: **PASS** executed, criterion met in the stated scope · **PARTIAL** the part testable in
E1 passed, the rest did not run · **NOT RUN** could have run, did not · **BLOCKED** cannot run here ·
**N/A** the feature does not exist in this branch · **FAIL** executed, not met.
A loopback test is not a network test; a build is not a runtime test; one platform's build says
nothing about another.

## Executed in E1

| ID | Command | Observed |
| --- | --- | --- |
| E1-1 | `cargo fmt --check` | clean |
| E1-2 | `cargo clippy --workspace --all-targets` (workspace lints: `clippy::all` deny, pedantic warn) | no warnings |
| E1-3 | `cargo test --workspace` | **172 passed, 0 failed** |
| E1-4 | `pnpm typecheck`, `pnpm lint`, `pnpm test` in `apps/client` and `packages/sdk-js` | clean; client 13 tests, `@ilmek/sdk` 20 tests |
| E1-5 | `scripts/smoke/local-processes.sh` | PASS: registry + host + devices as **separate OS processes**: join by profile file, publish, resolve, download, calls, host restart keeps data, backend `kill -9` restarted, a second device cannot take over the namespace |
| E1-6 | `scripts/smoke/multi-app.sh` (run 20 times in a row after the locking fix) | PASS every time: HOST-1 runs 3 applications, HOST-2 (own process and data) runs 2; data isolated; stopping one backend leaves others up; killing HOST-1 leaves HOST-2 reachable; HOST-1 returns with data |
| E1-7 | `scripts/smoke/backup-restore.sh` | PASS: live registry+host backed up, data directories destroyed, restored, restarted, state equals backup point, writes work; tampered backup and non-empty target refused |
| E1-8 | `crates/ilmek-cli/tests/node_flow.rs` (run 12 times in a row) | PASS: the real `ilmek` binary. (a) `ilmek node`; a second "machine" with its own `ILMEK_HOME` finds the node by UDP, joins, resolves and downloads an app published on the node; (b) joining by `--registry` address; (c) `ilmek host start` on another "machine" finds the node, registers itself, serves an app of its own, which a client then fetches; (d) a port edited in `registry.toml` is the port devices are told; (e) without a node the error tells the user what to do |
| E1-9 | `scripts/quickstart.sh` followed by `resolve`, `call … topics.create`, `call … topics.list`, and `join` + `fetch` from a second home directory | worked; output seen, not asserted by a script |

A bug found and fixed while doing this: with two processes on the same SQLite file (the `ilmek app …`
command and a running host), a transaction that started as a read and then wrote failed immediately
with `database is locked` (about 1 run in 10 of `multi-app.sh`). Write transactions now begin
`IMMEDIATE`.

## Acceptance for this branch

| ID | Criterion | Status | Evidence |
| --- | --- | --- | --- |
| D01 | One command starts a node (registry + application host) | PASS | E1-8 (a), E1-9 |
| D02 | A second device finds the node and joins with no file or code | PARTIAL | PASS over loopback with UDP probe/answer (E1-8, `crates/ilmek-core/tests/discovery.rs`). **Real LAN broadcast: NOT RUN** (needs a second computer) |
| D03 | An app published on the node can be resolved, downloaded and called from the second device | PARTIAL | PASS as separate processes on one machine (E1-8, E1-5). Cross-machine: NOT RUN |
| D04 | Another machine's host registers by itself and serves its own apps | PARTIAL | PASS as a separate process with its own home directory (E1-8 c). Cross-machine: NOT RUN |
| D05 | The host's address needs no configuration | PASS | registry test `a_host_that_sends_0_0_0_0_is_recorded_under_the_address_it_connected_from`; client tests for `host_address`; E1-8 |
| D06 | No special hardware (two Wi-Fi cards, AP mode) is required | PASS (by construction) | nothing in the code touches radios; the `ilmek-network` crate was removed |
| D07 | The desktop client joins automatically and opens apps | PARTIAL | UI tests pass (`finds the node by itself when nothing is typed`, `says what to do when no node answers`); the Rust shell compiles. **The window was not started**; macOS NOT RUN |
| D08 | Several applications on one host with independent lifecycles | PASS | E1-6; `a_crashing_backend_leaves_other_apps_running_and_recovers`, `stopping_one_app_makes_only_that_app_unavailable` |
| D09 | Data survives restarts; retried writes apply once | PASS | `data_survives_a_host_restart_and_the_dedupe_record_with_it`, SDK tests `a_write_with_the_same_operation_id_is_applied_once`, `parallel_duplicates_commit_exactly_once` |
| D10 | Damaged or interrupted downloads cannot corrupt an installed app | PASS | `a_tampered_package_is_rejected_and_leaves_nothing_behind`, `a_package_altered_on_the_host_is_rejected_by_the_client_and_never_cached`, `cancelled_downloads_resume_from_the_bytes_already_received`, `a_failed_update_never_disturbs_the_active_version` |
| D11 | Backup and restore of live data | PASS | E1-7 |
| D12 | Page isolation (content policy, bridge without host/path fields) | PARTIAL | `crates/ilmek-runtime/tests` (14) pass; the WebView probe script `scripts/e2e/isolation-probe.sh` was **not re-run** on this branch; other platforms NOT RUN |
| D13 | Security (encryption, identity, signatures) | **N/A** | removed on purpose; see [SECURITY](SECURITY.md) |

## Protocol conformance (PROTOCOL.md §20)

P01–P05, P07–P10, P13–P16, P19–P21, P23, P24 are covered by the tests named in earlier sections and
by `crates/ilmek-protocol/tests` (frames, strict JSON, ids, messages, manifest, fixtures) and
`crates/ilmek-core/tests/{session,transport}.rs`. P25 (discovery) and P26 (`0.0.0.0` host address)
are covered by `crates/ilmek-core/tests/discovery.rs` and the registry test above.
P06, P11, P12, P17, P18, P22 do not exist in this branch (**N/A**).
The wire fixtures of this branch are `tests/protocol/fixtures/wire_examples.ilmek.json`
(regenerated for the new schema; test `manifest_hash_matches_fixture` and the frame round-trip tests
use it). A byte-level check of every message type against an independent implementation does not exist.

## Not run, and what it needs

- A second physical computer (Mac) on the same network: real broadcast discovery, firewall
  behavior, mixed-OS builds.
- macOS, Windows and Android builds of the CLI and the desktop client.
- Starting the desktop client window; `scripts/e2e/isolation-probe.sh` (needs a display).
- The systemd units (`deploy/systemd`) as services.
- Any security property.
