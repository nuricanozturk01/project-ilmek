# Test report

**Revision:** `42167b8` plus uncommitted working-tree changes (nothing has been committed in this
session). **Date:** 2026-10-23.
**Environment E1:** the development PC in [HARDWARE_INVENTORY](HARDWARE_INVENTORY.md): Linux Mint 22.3,
x86_64, rustc 1.96.0, Node 24.17.0, pnpm 12.14.0, X11 display, loopback networking only.
**No other environment was available.** Nothing below ran on Wi-Fi, Android, Windows or macOS.

Statuses: **PASS** executed, criterion met in the stated scope · **PARTIAL** the software-testable
part passed, the part needing the physical setup did not run · **BLOCKED** cannot run, hardware or
platform not available · **NOT RUN** could have run, did not · **FAIL** executed, not met.
A loopback test is not a Wi-Fi test; a build is not a runtime test; one platform's build says
nothing about another.

## Executed in E1

| ID | Command | Observed |
| --- | --- | --- |
| E1-1 | `cargo fmt --all --check` | clean |
| E1-2 | `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| E1-3 | `cargo test --workspace` | **230 passed, 0 failed** (8 consecutive full runs before the last additions were all green; an earlier port-allocation flake was fixed in `ilmek-testing`) |
| E1-4 | `pnpm -r lint`, `typecheck`, `test`, `build` | clean; `@ilmek/sdk` 20 tests, client 11 tests |
| E1-5 | `scripts/smoke/local-processes.sh` | PASS: registry + host + devices as separate processes: enroll (reused token and wrong CA fingerprint refused), publish, resolve, download, calls, host restart keeps data, backend `kill -9` restarted, revoked member refused |
| E1-6 | `scripts/smoke/multi-app.sh` | PASS: HOST-1 runs 3 applications at once, HOST-2 (own publisher, process, data) runs 2; data isolated; stopping one backend leaves others up; killing HOST-1 leaves HOST-2 reachable; HOST-1 returns with data |
| E1-7 | `scripts/smoke/backup-restore.sh` (run 9 times) | PASS: live registry+host backed up, data directories destroyed, restored, restarted, state equals backup point, writes work; tampered backup and non-empty target refused |
| E1-8 | `scripts/e2e/isolation-probe.sh` | PASS: Linux/WebKitGTK, 2 probes × 29 checks, canary hits 0, storage gone after restart |
| E1-9 | `pnpm tauri build --bundles deb` | produced `target/release/bundle/deb/İlmek_0.1.0_amd64.deb` (6.3 MiB); **installed and launched: no**; AppImage not attempted (needs downloaded tooling) |
| E1-10 | `ilmek network plan --config configs/node-{a,b}.toml.example`; `apply` without `--yes` | plan printed, nothing changed, refusal as designed. `apply --yes` **never run** |

## Product acceptance A01–A24 (REQUIREMENTS §26)

| ID | Status | Evidence / what is missing |
| --- | --- | --- |
| A01 | BLOCKED | Nodes and end devices not provided; [HARDWARE_INVENTORY](HARDWARE_INVENTORY.md) |
| A02 | BLOCKED | Needs two real access points; plan/render logic tested on a fake machine only (`ilmek-network` tests) |
| A03 | BLOCKED | Needs the real transit link |
| A04 | BLOCKED | Needs both nodes routing |
| A05 | NOT RUN | No run with WAN and mobile data off on real devices |
| A06 | PARTIAL | Rust protocol/registry/host/client crates have no HTTP/WebSocket dependency (`cargo tree`); all E1 traffic is ILMEK/1 over TLS. No packet capture on a real network |
| A07 | PARTIAL | Resolve by name PASS (E1-5); not from PHONE-2 |
| A08 | PARTIAL | 3 applications on one host PASS (E1-6); not on HOST-1 hardware |
| A09 | PARTIAL | Second independent host with own publisher/data PASS (E1-6); not on HOST-2 hardware |
| A10 | PARTIAL | One member's comment/note visible to another PASS (E1-5, `a_comment_from_one_member_is_visible_to_another`); not from a phone |
| A11 | PARTIAL | Data survives host restart and backend kill PASS (E1-5, E1-6, `data_survives_a_host_restart_and_the_dedupe_record_with_it`) |
| A12 | PARTIAL | `a_write_with_the_same_operation_id_is_applied_once`, `parallel_duplicates_commit_exactly_once`, `repeating_a_write_with_its_operation_id_posts_once`; not over Wi-Fi |
| A13 | PARTIAL | E1-6 and `a_crashing_backend_leaves_other_apps_running_and_recovers` PASS |
| A14 | PARTIAL | E1-6 (kill HOST-1, HOST-2 reachable) PASS; not on two machines |
| A15 | BLOCKED | Needs a real transit loss |
| A16 | PARTIAL | `a_tampered_package_is_rejected_and_leaves_nothing_behind`, `a_package_altered_on_the_host_is_rejected_by_the_client_and_never_cached` PASS |
| A17 | PARTIAL | `updates_replace_the_published_version_and_failed_updates_change_nothing`, `a_failed_update_never_disturbs_the_active_version` PASS |
| A18 | PARTIAL | `only_the_namespace_owner_can_publish_update_or_unregister`, `other_principals_cannot_read_change_or_even_detect_a_private_note` PASS |
| A19 | PARTIAL | Linux/WebKitGTK PASS (E1-8). Android WebView, WebView2, WKWebView NOT RUN |
| A20 | BLOCKED | No Android client exists yet (not started, by decision); Windows/macOS never built or run. Linux desktop client built (E1-9) and driven by E1-8 only |
| A21 | PARTIAL | `names_resolve_from_the_cache_while_the_registry_is_down_but_only_for_known_names` PASS; not with a real registry outage on devices |
| A22 | PASS | E1-7, loopback processes on E1 |
| A23 | PARTIAL | `cancelled_downloads_resume_from_the_bytes_already_received`, `the_cache_quota_refuses_packages_that_do_not_fit`, client hostile-host tests PASS; not over Wi-Fi |
| A24 | PASS | `revoked_members_and_revoked_hosts_are_shut_out`, E1-5; offline window (24 h) documented in [SECURITY](SECURITY.md) |

## Protocol conformance P01–P24 (PROTOCOL.md §20)

All in E1, in-process or loopback; none on a real network. Fixtures from `WIRE_EXAMPLES.json` are
imported (`crates/ilmek-protocol/tests/fixtures.rs`, `wire.rs`; decisions 0001/0002). Fixture checks
do not validate the full state machine; that is what the session/host tests below are for.

| ID | Status | Tests |
| --- | --- | --- |
| P01 | PASS | `prefix_is_magic_then_big_endian_lengths`, `valid_frames_round_trip_byte_for_byte` |
| P02 | PASS | `byte_at_a_time_delivery_yields_the_same_frame`, `reader_reassembles_frames_from_small_writes`, `any_chunking_of_a_frame_sequence_decodes_identically` |
| P03 | PASS | `two_frames_in_one_read_are_split` |
| P04 | PASS | `oversize_lengths_fail_without_waiting_for_payload`, `limit_sized_frame_is_accepted_and_one_more_byte_is_not_encodable` |
| P05 | PASS | `rejects_other_versions_nil_and_non_canonical_forms`, `rejects_duplicate_keys_even_when_nested`, `rejects_bom_and_bad_utf8`, `unknown_data_fields_are_rejected` |
| P06 | PASS | `ilmek-core/tests/tls.rs`: wrong ALPN, other CA, other IP, wrong peer, expired, non-TLS |
| P07 | PASS | `app_call_before_hello_is_rejected_and_the_connection_ends` |
| P08 | PASS | `replies_are_matched_by_request_id_when_they_arrive_out_of_order` |
| P09 | PASS | `parallel_duplicates_commit_exactly_once`, `concurrent_duplicate_writes_commit_once_and_conflicting_reuse_is_refused` |
| P10 | PASS | same registry test; SDK `a_write_with_the_same_operation_id_is_applied_once` |
| P11 | PASS | `a_lost_enrollment_reply_can_be_replayed_but_not_altered` |
| P12 | PASS | `permissions_versions_and_forged_hashes_are_enforced_by_the_host`, `cursors_are_bound_to_principal_and_topic_and_topics_are_authorized`, notes privacy test |
| P13 | PASS | client: `chunk_sequence_offset_size_and_total_deviations_are_rejected`, `a_host_that_ignores_the_resume_offset_is_rejected`, `bytes_that_do_not_match_the_declared_hash_are_never_accepted`; host: `asset_requests_that_disagree_with_the_manifest_are_refused` |
| P14 | PASS | `cancel_ends_a_transfer_with_exactly_one_terminal_reply`, `cancel_after_completion_reports_already_finished_and_unknown_ids_not_found` |
| P15 | PASS | `the_ack_window_limits_in_flight_events`, `a_consumer_that_stops_acknowledging_is_cut_off`, `writer_applies_backpressure_instead_of_queueing_without_bound` |
| P16 | PASS | `pruned_history_makes_old_cursors_expire`, `events_follow_commits_and_resume_from_a_cursor_without_loss_or_repeats` |
| P17 | PASS | `a_snapshot_takes_effect_only_after_its_last_page`, `nothing_is_authorized_before_the_first_complete_snapshot` |
| P18 | PASS | `a_host_with_stale_trust_refuses_work_with_trust_stale`, `a_client_whose_own_trust_is_stale_does_not_even_connect`, `a_clock_that_moves_backwards_makes_the_snapshot_stale` |
| P19 | PASS | `a_failed_update_never_disturbs_the_active_version`, `a_tampered_package_is_rejected_and_leaves_nothing_behind` |
| P20 | PASS | `message_types_are_confined_to_their_connection_mode` |
| P21 | PASS | `only_the_namespace_owner_can_publish_update_or_unregister`, `publishing_needs_the_publisher_role_a_granted_host_and_a_valid_key` |
| P22 | PASS | `revoking_a_peer_ends_idle_sessions_and_rejects_new_requests`, `revocation_is_applied_to_the_very_next_request`, `a_revoked_device_is_cut_off_and_its_certificate_is_published` |
| P23 | PASS | `a_stalled_bulk_transfer_does_not_delay_the_control_connection` (control reply < 1 s while a 24 MB bulk reader is stalled), `a_second_transfer_on_one_bulk_connection_is_refused`. Loopback only: real-link latency not measured |
| P24 | PASS | `invalid_bridge_calls_never_reach_the_server`, `the_page_cannot_name_an_application_host_or_path`, `a_package_only_gets_the_capabilities_its_manifest_declares`; E1-8 on WebKitGTK. Other platforms' WebViews not covered |

## Other checks requested by the instructions

| Area | Status | Evidence |
| --- | --- | --- |
| Partial/combined reads, invalid metadata, oversize, EOF, timeout | PASS | `ilmek-protocol/tests/framing.rs`, `ilmek-core/tests/transport.rs` |
| Registry ops, name ownership, trust snapshots, cache expiry | PASS | `ilmek-registry/tests`, `ilmek-core/tests/trust.rs` |
| Duplicate/conflicting writes, connection loss around commits | PASS | SDK and host tests above |
| Cert renewal and registry identity re-issue | PASS | `ilmek-registry/tests/renewal.rs` (no real 30-day wait) |
| Database migrations, backup, restore | PASS | `ilmek-store/tests/{store,backup}.rs`, E1-7 |
| Node network plan/apply/rollback logic | PASS (fake machine) | `ilmek-network/tests/node.rs`, `ilmek-cli/tests/network.rs`. Real machine: NOT RUN |
| systemd units | PARTIAL | `systemd-analyze verify` only; never started as services |
| Performance (REQUIREMENTS §27) | NOT RUN | Needs the real link; no throughput or latency figure is claimed |

## Defects and gaps found while testing

- Fixed: `ilmek-testing` handed out the same port twice / lost races, causing roughly one failed
  workspace run in two (`AddrInUse`).
- Fixed: backup copies kept SQLite WAL mode and grew side files when verified.
- Fixed: a plan-text test matched an explanatory note (assertion narrowed to the steps).
- Open: Windows ACL hardening of key files; no Android client; no Windows/macOS build; AppImage
  not built; `.deb` not installed; unit files not started; no CI configuration in the repository.
