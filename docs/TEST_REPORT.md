# Test report

**Branch:** `feat/simple-no-security`. **Revision:** `6f63019` plus the documentation, `.gitignore`
and CLI help-text changes of the working tree. **Date:** 2026-11-01.
**Environment E1:** one Linux x86_64 development PC (Linux 7.0, rustc 1.96.0, Bun 1.3.14, Node
24.17.0, OpenJDK 25.0.4.1.1, Gradle 9.4.1), loopback networking only. **No other environment was
available.** Nothing below ran on a second computer, a real LAN, macOS, Windows or Android.

Statuses: **PASS** executed, criterion met in the stated scope · **PARTIAL** the part testable in
E1 passed, the rest did not run · **NOT RUN** could have run, did not · **N/A** the feature does not
exist in this branch · **FAIL** executed, not met. A loopback test is not a network test; a build
is not a runtime test; one platform's result says nothing about another.

## Executed in E1

| ID | Command | Observed |
| --- | --- | --- |
| E1-1 | `cargo fmt --check` | clean |
| E1-2 | `cargo clippy --workspace --all-targets` | no warnings |
| E1-3 | `cargo test --workspace` | **187 passed, 0 failed** |
| E1-4 | `bun run typecheck`, `bun run lint`, `bun run build` | clean in all three workspaces |
| E1-5 | `bun run test` | `@ilmek/sdk` 21, `@ilmek/angular-http` 27, `@ilmek/client` 14 passed |
| E1-6 | `scripts/smoke/local-processes.sh` | PASS: registry, host and devices as **separate OS processes**: join by profile file, publish, resolve, download, calls, host restart keeps data, backend `kill -9` is restarted, a second device cannot take over a namespace |
| E1-7 | `scripts/smoke/multi-app.sh` | PASS: host 1 runs 3 applications, host 2 (own process and data) runs 2; data isolated; stopping one backend leaves the others up; killing host 1 leaves host 2 reachable; host 1 returns with its data |
| E1-8 | `scripts/smoke/backup-restore.sh` | PASS: live registry and host backed up, data directories destroyed, restored, restarted; state equals the backup point and writes work; a tampered backup and a non-empty target are refused |
| E1-9 | `crates/ilmek-cli/tests/node_flow.rs` (part of E1-3) | PASS with the real `ilmek` binary: (a) `ilmek node`, a second "machine" with its own `ILMEK_HOME` finds it by UDP, joins, resolves and downloads an app; (b) join by `--registry`; (c) `ilmek host start` on another "machine" registers itself and serves an app that a client fetches; (d) a port edited in `registry.toml` is the port devices are told; (e) without a node the error says what to do |
| E1-10 | `crates/ilmek-core/tests/discovery.rs`, `node_flow.rs::a_device_joins_without_udp_by_address_and_by_scanning_the_network` (part of E1-3) | PASS: the community is learned from the registry's TCP port alone (a host answers "not a registry"); a TCP scan finds the registry among 40 dead addresses; with UDP blocked both `join --registry IP:PORT` and a plain `join` work. A scan on a real LAN was not run |
| E1-11 | Java: `./gradlew --rerun-tasks :starter:test :web-adapter:test` in `java/ilmek-spring-boot-starter`; `./gradlew --rerun-tasks test` in `examples/tasks/backend-java` and `examples/rest-notes/backend-java` (default Boot 4.1.2, Java 25) | PASS: starter 131, web adapter 33, tasks 22, rest-notes 11 tests, 0 failures, 0 skipped |
| E1-12 | Angular: `npm test` in `examples/tasks/frontend-angular` and `examples/rest-notes/frontend-angular` | PASS: 23 and 11 tests |
| E1-13 | `crates/ilmek-testing/tests/{java_backend_e2e,rest_notes_e2e}.rs` (part of E1-3) | PASS: real host with the real Spring Boot jars: plain read/write, idempotent retry, `OPERATION_CONFLICT`, subscription event, `SIGKILL` of the backend and replay; REST adapter CRUD, and the jar holds no listening TCP socket |

Executed earlier on this branch and not repeated on the final tree:

| ID | Procedure | Observed |
| --- | --- | --- |
| E0-1 | Java 17/21/25 × Boot 3.5.16/4.1.2 matrix of the starter, web-adapter and `tasks` tests | all cells passed |
| E0-2 | `scripts/quickstart.sh`, then `resolve`, `call … topics.create`, `call … topics.list`, and `join` + `fetch` from a second home directory | worked; output seen, not asserted by a script |
| E0-3 | Desktop client on Linux (debug build, real display): `ilmek-client ilmek://nuri/merhaba`, `xdg-open ilmek://nuri/forum` against a node in another process | observed, not scripted: the second start exits 0 and the running instance opens the app; screenshots show the static page and the forum (SDK bridge working) |
| E0-4 | Desktop client as browser tabs (one webview, sandboxed iframes): opened `ilmek://nuri/forum` and `ilmek://nuri/notlar`, listed forum topics, created a note | observed, not scripted: content fills the tab, styles and scripts load, SDK calls work through `postMessage`. Not observed: dark theme, tab discard after 10 minutes or at a 5th tab, Windows/macOS, `http://ilmekapp.localhost` form of the scheme. Twice the first load stalled for tens of seconds with the window off-screen; not reproduced, cause unknown |
| E0-5 | `tasks` example in the desktop client | not observed: an attempt produced no usable screenshot |

A bug found while testing: with two processes on one SQLite file (an `ilmek app …` command and a
running host) a transaction that started as a read and then wrote failed with `database is locked`
(about 1 in 10 runs of `multi-app.sh`). Write transactions now begin with `BEGIN IMMEDIATE`.

## Acceptance for this branch

| ID | Criterion | Status | Evidence |
| --- | --- | --- | --- |
| D01 | One command starts a node (registry + application host) | PASS | E1-9 (a), E0-2 |
| D02 | A second device finds the node and joins with no file or code | PARTIAL | PASS over loopback with UDP probe/answer (E1-9, E1-10). **Real LAN broadcast: NOT RUN** (needs a second computer) |
| D03 | An app published on the node can be resolved, downloaded and called from the second device | PARTIAL | PASS as separate processes on one machine (E1-6, E1-9). Cross-machine: NOT RUN |
| D04 | Another machine's host registers by itself and serves its own apps | PARTIAL | PASS as a separate process with its own home directory (E1-9 c). Cross-machine: NOT RUN |
| D05 | The host's address needs no configuration | PASS | registry test `a_host_that_sends_0_0_0_0_is_recorded_under_the_address_it_connected_from`; client tests for `host_address`; E1-9 |
| D06 | No special hardware (two Wi-Fi cards, AP mode) is required | PASS (by construction) | nothing in the code touches radios |
| D07 | The desktop client joins automatically and opens apps | PARTIAL | UI tests (`finds the node by itself when nothing is typed`, `says what to do when no node answers`) pass; on Linux the window opens apps from a node in another process (E0-3, E0-4). The **Find and join** button itself was not clicked; macOS/Windows NOT RUN |
| D08 | Several applications on one host with independent lifecycles | PASS | E1-7; `a_crashing_backend_leaves_other_apps_running_and_recovers`, `stopping_one_app_makes_only_that_app_unavailable` |
| D09 | Data survives restarts; retried writes apply once | PASS | `data_survives_a_host_restart_and_the_dedupe_record_with_it`; SDK tests `a_write_with_the_same_operation_id_is_applied_once`, `parallel_duplicates_commit_exactly_once` |
| D10 | Damaged or interrupted downloads cannot corrupt an installed app | PASS | `a_tampered_package_is_rejected_and_leaves_nothing_behind`, `a_package_altered_on_the_host_is_rejected_by_the_client_and_never_cached`, `cancelled_downloads_resume_from_the_bytes_already_received`, `a_failed_update_never_disturbs_the_active_version` |
| D11 | Backup and restore of live data | PASS | E1-8 |
| D12 | Page isolation (content policy, bridge without host/path fields) | PARTIAL | `crates/ilmek-runtime/tests` (15 tests) pass. `scripts/e2e/isolation-probe.sh` targets the old window-per-app model and was **not re-run**; other platforms NOT RUN |
| D13 | `ilmek://…` links open in the client (browser hand-off) | PARTIAL | Linux: `xdg-open` and a second program start are forwarded to the running instance (E0-3). **Chrome was not used**; the macOS `.app` build and URL-scheme registration NOT RUN; Windows NOT RUN |
| D14 | Spring Boot + Angular applications | PARTIAL | backends, adapter, frontends and real host + jar e2e PASS (E1-11 to E1-13). Installing them on a node with `app install/set-backend/start/publish` and viewing them in the client: NOT RUN |
| D15 | Security (encryption, identity, signatures) | **N/A** | removed on purpose; see [SECURITY](SECURITY.md) |

## Protocol conformance ([PROTOCOL.md](../PROTOCOL.md) §20)

P01–P05, P07–P10, P13–P16, P19–P21, P23, P24 are covered by `crates/ilmek-protocol/tests` (frames,
strict JSON, ids, messages, manifest, fixtures), `crates/ilmek-core/tests/{session,transport}.rs` and
the host, SDK and runtime tests named above. P25 (discovery) and P26 (`0.0.0.0` host address) are
covered by `crates/ilmek-core/tests/discovery.rs` and the registry test named in D05.
P06, P11, P12, P17, P18 and P22 do not exist in this branch (**N/A**).

Wire fixtures: `tests/protocol/fixtures/wire_examples.ilmek.json` (regenerated for this branch's
schema; used by `manifest_hash_matches_fixture` and the frame round-trip tests). A byte-level check
of every message type against an independent implementation does not exist.

## Not run, and what it needs

- A second physical computer on the same network: real broadcast discovery, firewall behavior,
  mixed-OS builds.
- macOS, Windows and Android builds of the CLI and the desktop client.
- `scripts/e2e/isolation-probe.sh` (needs a display and an update for the iframe model).
- The systemd units in `deploy/systemd` as services.
- Installing the Java/Angular examples on a node and viewing them in the desktop client.
- Any security property.
