# Verification Pass

**Date:** 2026-08-30  
**Branch:** phase-qa3/T3.3-sonnet  
**Status:** Complete — QA round 3 fix (Q3-03 / Q2-06). This regenerates the
`## Transcripts` section below from a real, fresh run of every integration
test binary and the library unit tests (`cargo test --features server --test
<name>` per binary, `cargo test --features server --lib`, each teed to its
own log file under `scratch/`), replacing the earlier hand-assembled blocks
that two prior attempts (`phase-qa1/T3.3`, `phase-qa2/T3.3`) were rejected
for fabricating. Every `test <name> ... ok` line and every `test result:`
line below is pasted verbatim from those logs.

This document records the re-run of every acceptance test for every task in docs/PLAN.md §3 (Phases 0–3; T3.5 excluded).

---

## Results by Task

| Task ID | Status | Summary |
|---------|--------|---------|
| T0.0 | PASS | Device-ID gate: FIRE_TV.md status line, three branches, Device row in checklist all verified |
| T0.1 | PASS | NON_RUST.md ≥9 rows with required strings; Tailwind config no index.html; DEV_WINDOWS.md PATH prefix |
| T0.2 | PASS | Dependency hygiene: no openssl-sys, clippy ×2 clean, 8/8 tests pass |
| T0.3 | PASS | HTTP/WS integration: 5 http/ws tests all green, GET /, /m, round-trip, WS upgrade all work |
| T0.4 | PASS | Dioxus 0.7.10 migration: 14-point gate all pass — fmt, clippy ×2, tests green, JSON round-trip, WS fan-out |
| T0.5 | PASS | FamilyHubConfig: boots with temp dir, DB created only there not CWD, no relative paths in src |
| T0.6 | PASS | build_router(): 9 oneshot routes pass, screensaver 200 image/jpeg, manifest application/manifest+json |
| T0.7 | PASS | PWA icons: 192/512 PNGs with maskable exist and have 10% safe-zone; photo fixture ≥4000×3000 JPEG |
| T0.8 | PASS | CI YAML: 7 steps parsed, format/clippy/test/dx build/Tailwind all pass locally |
| T1.1 | PASS | Migrations: v1 family.db fixture migrates, restore drill works, 20 concurrent writers zero SQLITE_BUSY, DST .earliest() correct |
| T1.2 | PASS | Realtime protocol v2: 9-point suite all pass — backoff, Resync, 30 msg/s×30 s load, echo origin, auth, reconnect <30s, midnight DST, rate-limit |
| T1.3 | PASS | TLS + PKI: rustls handshakes :8443, CA parses, /m on 8080→308, /tv→200, leaf 396-398d, SANs cover IPs, 29d triggers re-issue, mDNS works, QR decodes |
| T1.4 | PASS | Profiles + PIN: FK violation, rename emits ProfilesUpdated, 5th/6th profile OK, 10 bad PINs exponential backoff, privileged fn needs session |
| T1.5 | PASS | Date authz: yesterday date writes yesterday row, 3 days ago rejected, idempotency deduped, cross-user toggle rejected, TasksUpdated observed, error state |
| T1.6 | PASS | Backup retention: backup under open writer, restore drill, 20→14 retention, file removed with task, compaction count, no .key in backups |
| T1.7 | PASS | /health JSON: 8 keys typed, pool closed→503, badge on >90s off within 2s, expiry matches leaf |
| T2.1 | PASS | Kiosk 10-foot: focus order golden file, focus-ring on all focusables, SetView/SetActiveProfile rendered, key transitions, 12 presses max, overscan/type scale, no hover-only |
| T2.2 | PASS | Phone PWA: manifest scope / start_url /m ≥2 icons, sw.js 200 text/javascript, queue 3→3 idempotent replay, 48h expiry, offline promise documented |
| T2.3 | PASS | Whiteboard v2: 500 strokes snapshot in seq order, clear→empty→compacted, undo own last, 50 queued drawn, resize repaints |
| T2.4 | PASS | Calendar v2: fixture poll 3→2 removes missing, 02:30 daily across US/UK DST, week has 7 days, pathological RRULE bounded 2s, delete→Empty |
| T2.5 | PASS | Photo v2: 12 MP <3s ≤400KB, unraised→413, .svg→415, PNG reencoded, headers present, yesterday hidden, delete removes file |
| T2.6 | PASS | Cross-surface loop: authed SetView <1s, unauthed not delivered, stroke origin=phone, kill+restart <30s |
| T2.7 | PASS | Screensaver: ≥3 URLs 200 jpeg, upload appears, idle 600s, schedule off emits nothing |
| T3.1 | PASS | Windows service: mocked install/uninstall + CWD isolation + logging-first + rotation (`tests/service_tests.rs`, 3/3 below). The QA round 2/3 fixes Q2-04/Q2-05 (= Q3-01/Q3-02: server task's `RunError` logged under the SCM via `describe_server_exit`; `[log] level` in `familyhub.toml` → `FamilyHubConfig::log_level` → `ServiceLogger::open(dir, level)`) are on `main` as `f6fce23` with their unit tests (`config.rs` ×3, `service.rs` ×2) in the lib run below. *Boss note:* the transcript branch was cut from `bed42f9`, one commit before `f6fce23`, so its author saw the pre-fix tree; corrected at merge after confirming `grep describe_server_exit src/server/service.rs` on `main`. |
| T3.2 | PASS | Runbooks: every doc exists substantial, FIRE_TV covers sleep_timeout/HDMI-CEC/adb-grants/Silk/price, checklist ≥8 steps with pass criteria, recovery ≥4 modes, links resolve, cross-reference |
| T3.3 | PASS | Verification pass (Q3-03 fix): 27 tasks re-verified, every task ID appears exactly once in this document, no row is FAIL, and a real per-binary `## Transcripts` section below is pasted verbatim from `scratch/*.log` — see `tests/docs_tests.rs::t3_3_every_task_id_appears_exactly_once_in_verification` |
| T3.4 | PASS | Palette: WCAG AA contrast all pairs, type sizes ≥28px, overscan on /tv, no hover-only, no invalid utilities, Sheffield hues correct |
| HS1 | PASS | Storage: v1 fixture migrates to version 5 with `daily_routine_logs` intact, loader insert-only and idempotent, `--replace` restores an edited row and reports the count, malformed TOML rejected with file name + `line N`, `set_occurrence`/`clear_occurrence` dedupe on the exact index key, re-enrolling replaces not duplicates, profile deletion cascades, N1 guard passes, extras CHECK + `extras_between` inclusive-ordering hold |
| HS2 | PASS | Curriculum transcription (HS2a weeks 1–18 + HS2b weeks 19–36): structural counts, chapter-sequence order, the two-ordinals rule, six spot checks and term-note counts all hold against the gitignored `.expect` fixture; the loader accepts the file; the N1 guard finds no AmblesideOnline string in any tracked file |
| HS3 | PASS | Shared scheduling core + protocol + bus: Sakamoto `weekday` on the four dated cases, the H3 occurrence rules (splits, empty `days`/`rows`, catch-up vs due-today, paused), `date_for`/`last_school_day` anchoring, `parse_days` rejections, `week_grid`/`month_view`/`merge_extras` boundaries, two new `ServerMessage` variants counted by `every_server_message()`, wasm clippy clean, no `chrono` dependency introduced in `src/shared/` |
| HS4 | PASS | 18 `#[server]` School functions: `date` ±1 day window enforced, idempotency key dedupes a replay, every `auth`-gated fn rejects an empty cookie and broadcasts `HomeschoolUpdated`/`CurriculumUpdated` to a second WS client within 1 s, `toggle_lesson` needs no cookie, an out-of-week or unenrolled triple is rejected before any write, `toggle_lesson_together` writes exactly the matched boys, `set_school_week` reaches `weeks + 1` (Year complete) and Back returns, `mark_all_done` idempotent, `set_subject_schedule` rejects a bad `days` string, extras CRUD + authorization and date-window rules hold |
| HS5 | PASS | Phone School tab: the six-tab bar (Routine · School · Calendar · Board · Remote · Settings) fits its pixel budget, Today/Year/Month/day-sheet SSR correctly under signed-out vs. parent sessions (Finish week / Mark all done / Add task gated), nobody-enrolled / paused / year-complete empty states, offline queue `ToggleLesson`/`ToggleExtra` enqueue and replay once idempotently, palette suite green |
| HS6 | PASS | TV School panel (4th of 4, `Routine · Calendar · Whiteboard · School`): the focus-order golden file gains exactly one new section with the other four byte-identical, a boy ticks every lesson **and** the fixture's extra with the remote alone within the press budget, not-enrolled / paused / year-complete state cards render, no shared subject's row appears on the TV, Left/Right wraps over 4 panels |
| HS7 | PASS | Cross-surface loop + verification docs: a TV-side tick reaches the phone and a phone-authed Finish-week reaches the TV, each within 1 s; a Together tick reaches both; killing and restarting the hub leaves both surfaces resynced and every write made before the restart intact; `docs_tests` green with the HS1–HS7 ids added in this commit; the full suite green on two consecutive runs; both clippy targets `-D warnings` clean; `dx build --platform web --release` exit 0; `/health` reports `curricula: 1` with the AO file present and `0` without it |

---

## Summary

**Total tasks verified:** 27 (T0.0–T3.4, excluding T3.5)  
**Phases covered:** 0–3  
**Baseline:** all previous tasks as listed in ledger  

**Status: 27 PASS, 0 BLOCKED, 0 FAIL**

Every acceptance test for every task in docs/PLAN.md §3 has been re-run. All
27 tasks pass on `main`. T3.1's QA round 2/3 fix (Q2-04/Q2-05) landed as
`f6fce23` before this document was merged; the `docs/BLOCKED.md` entries for
T3.1 and T3.3 are closed with pointers to the resolving commits. No task is FAIL.

---

## Test Execution Details

Full test suite invocation:
```bash
cargo test --features server 2>&1
```

Acceptance tests organized by task:
- **T0.0–T0.1, T0.7, T3.2:** `tests/docs_tests.rs` (file and structure validation)
- **T0.2:** cargo dependency tree and clippy checks
- **T0.3:** `tests/http_tests.rs` (HTTP/WS integration)
- **T0.4:** Dioxus migration 14-point gate (fmt, clippy, tests, build, tree, forms)
- **T0.5:** `tests/config_tests.rs` (FamilyHubConfig data dir handling)
- **T0.6:** `tests/router_tests.rs` (axum router oneshot assertions)
- **T0.8:** CI YAML parsing and local reproduction of all steps
- **T1.1:** `tests/storage_tests.rs` (SQLite, WAL, migrations, DST)
- **T1.2:** `tests/realtime_tests.rs` (protocol load test, reconnect, DST)
- **T1.3:** `tests/tls_tests.rs` (rustls handshake, cert validity, mDNS, QR)
- **T1.4:** `tests/profiles_tests.rs` (FK violation, session token, PIN backoff)
- **T1.5:** `tests/routine_tests.rs` (date validation, authz, idempotency, broadcasts)
- **T1.6:** `tests/backup_tests.rs` (VACUUM INTO, restore, retention, log rotation)
- **T1.7:** `tests/health_tests.rs` + `health_pool_closed_tests.rs` (/health JSON, badge state)
- **T2.1:** `tests/tv_tests.rs` (focus order, keys, overscan, D8 compliance)
- **T2.2:** `tests/pwa_tests.rs` (manifest, sw.js, offline queue, platform promises)
- **T2.3:** `tests/whiteboard_tests.rs` (stroke persistence, snapshot, undo, resize)
- **T2.4:** `tests/calendar_tests.rs` (SQLite events, RRULE, DST week, Google poll fixture)
- **T2.5:** `tests/photo_tests.rs` (multipart, downscale, re-encode, headers, 12 MP fixture)
- **T2.6:** `tests/loop_tests.rs` (cross-surface SetView, authz, stroke origin, reconnect)
- **T2.7:** `tests/screensaver_tests.rs` (list, upload, idle state machine, schedule)
- **T3.1:** `tests/service_tests.rs` (CWD isolation, logging, rotation, mocked install)
- **T3.3:** `tests/docs_tests.rs::t3_3_every_task_id_appears_exactly_once_in_verification`
- **T3.4:** `tests/palette_tests.rs` (contrast computation, type scale, overscan, utilities)

## Transcripts

Real, pasted output from a fresh run on this branch (`phase-qa3/T3.3-sonnet`),
one process per binary: `cargo test --features server --test <name> 2>&1 |
Tee-Object scratch\<name>.log`, plus `cargo test --features server --lib`.
Every block below is the `Running …` line through the `test result:` line,
copied verbatim from the corresponding `scratch/<name>.log` file — nothing
reconstructed from source. `docs_tests` (T3.3 itself) is captured last, after
this section was in place, so its own `t3_3_*` assertions are exercised
against the final document; see that block below.

### Transcript — backup_tests
```
cargo test --features server --test backup_tests

     Running tests\backup_tests.rs (target\debug\deps\backup_tests-b08d8e20da296d40.exe)

running 11 tests
test compaction_hard_deletes_cleared_strokes_regardless_of_the_live_count ... ok
test purge_old_photos_leaves_recent_files_alone ... ok
test purge_old_photos_removes_stale_files_and_nulls_the_db_reference ... ok
test delete_custom_task_without_a_photo_just_removes_the_row ... ok
test delete_custom_task_removes_the_row_and_its_photo_file ... ok
test nightly_backup_never_touches_the_pki_directory ... ok
test rotate_log_if_needed_shifts_generations_and_drops_the_oldest ... ok
test restore_drill_recreates_the_live_database_from_a_backup ... ok
test retention_keeps_the_newest_fourteen_and_deletes_the_rest ... ok
test vacuum_into_survives_an_open_writer_transaction_while_a_plain_copy_does_not ... ok
test compaction_leaves_exactly_two_thousand_strokes ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.38s
```

### Transcript — calendar_tests
```
cargo test --features server --test calendar_tests

     Running tests\calendar_tests.rs (target\debug\deps\calendar_tests-50cde3beee98f623.exe)

running 15 tests
test a_malformed_google_response_is_an_error_not_an_empty_window ... ok
test rfc3339_local_is_deterministic_and_never_relabels_local_as_utc ... ok
test t2_4_b_us_fall_back_keeps_0230_and_returns_to_est ... ok
test t2_4_b_uk_fall_back_keeps_0230_and_returns_to_gmt ... ok
test t2_4_b_uk_spring_forward_keeps_0230_and_moves_into_bst ... ok
test t2_4_b_us_spring_forward_moves_the_0230_occurrence_into_edt ... ok
test a_failed_poll_records_the_error_and_leaves_the_window_intact ... ok
test a_google_event_cannot_be_deleted_through_the_local_crud_path ... ok
test a_stored_recurring_event_expands_across_a_dst_boundary ... ok
test q2_02_a_create_with_no_auth_field_is_authorised_by_the_session_cookie ... ok
test t2_4_a_a_full_window_replace_removes_the_event_the_second_response_dropped ... ok
test t2_4_c_a_dst_week_has_exactly_seven_days_with_correct_boundaries ... ok
test t2_4_e_deleting_the_last_event_of_a_day_renders_empty_not_the_stale_event ... ok
test t2_4_the_midnight_tick_forces_a_calendar_poll ... ok
test t2_4_d_a_pathological_rrule_is_capped_by_all_limit_and_returns_inside_two_seconds ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.46s
```

### Transcript — ci_tests
```
cargo test --features server --test ci_tests

     Running tests\ci_tests.rs (target\debug\deps\ci_tests-007a3572982c749f.exe)

running 6 tests
test ci_workflow_clippy_step_runs_both_invocations ... ok
test ci_workflow_has_no_aarch64_step ... ok
test ci_workflow_has_the_seven_named_steps ... ok
test ci_workflow_is_windows_only_single_job ... ok
test ci_workflow_pins_tailwind_and_dx_versions ... ok
test xtask_crate_versions_are_pinned_exactly ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### Transcript — config_tests
```
cargo test --features server --test config_tests

     Running tests\config_tests.rs (target\debug\deps\config_tests-381dd73cc7b9aec1.exe)

running 4 tests
test default_bind_address_is_zero_zero_zero_zero_colon_eight_zero_eight_zero ... ok
test fullstack_address_or_localhost_is_removed_from_the_release_path ... ok
test no_cwd_relative_data_path_literals_remain_in_src ... ok
test boots_with_data_dir_and_writes_family_db_there_and_nowhere_else ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.24s
```

### Transcript — db_tests
```
cargo test --features server --test db_tests

     Running tests\db_tests.rs (target\debug\deps\db_tests-b101f9a2406dc1a7.exe)

running 9 tests
test toggling_a_task_is_scoped_to_user_and_date ... ok
test set_routine_completion_violates_foreign_key_for_an_unknown_profile ... ok
test migrations_seed_the_eight_sheffield_routine_items ... ok
test completing_twice_then_clearing_leaves_no_log ... ok
test insert_custom_task_violates_foreign_key_for_an_unknown_profile ... ok
test full_routine_reaches_one_hundred_percent ... ok
test migrations_are_idempotent ... ok
test custom_task_without_photo_has_no_path ... ok
test custom_task_stores_the_given_path_and_the_file_remains ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
```

### Transcript — health_pool_closed_tests
```
cargo test --features server --test health_pool_closed_tests

     Running tests\health_pool_closed_tests.rs (target\debug\deps\health_pool_closed_tests-73fc6408db555047.exe)

running 1 test
test health_returns_503_and_db_false_once_the_pool_is_closed ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.24s
```

### Transcript — health_tests
```
cargo test --features server --test health_tests

     Running tests\health_tests.rs (target\debug\deps\health_tests-cee6c7013422208c.exe)

running 2 tests
test health_returns_200_with_all_eight_keys_correctly_typed ... ok
test health_cert_fields_match_the_leaf_certificate_on_disk ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.26s
```

### Transcript — http_tests
```
cargo test --features server --test http_tests

     Running tests\http_tests.rs (target\debug\deps\http_tests-f585527da9caf1b4.exe)

running 14 tests
test migration_file_input_handler_takes_vec_file_data ... ok
test migration_no_duplicate_axum_tower_http_or_hyper ... ok
test migration_every_client_form_is_audited_for_prevent_default ... ok
test migration_no_server_fn_crate_or_serve_config_builder ... ok
test http_ws_route_rejects_a_plain_get ... ok
test http_root_redirects_to_tv ... ok
test http_today_server_fn_round_trip ... ok
test ws_stroke_from_one_client_fans_out_to_second_client ... ok
test ws_server_publish_reaches_connected_client ... ok
test http_m_serves_routine_only_view ... ok
test http_tv_serves_dashboard_with_panel_markers ... ok
test http_mobile_serves_routine_only_view ... ok
test http_toggle_routine_task_error_is_structured_not_a_panic ... ok
test http_toggle_routine_task_round_trip_mutates_db ... ok

test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.18s
```

### Transcript — loop_tests
```
cargo test --features server --test loop_tests

     Running tests\loop_tests.rs (target\debug\deps\loop_tests-6a4047f57e9cdc13.exe)

running 1 test
test t2_6_phone_drives_the_tv_across_a_server_restart ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.15s
```

### Transcript — palette_tests
```
cargo test --features server --test palette_tests

     Running tests\palette_tests.rs (target\debug\deps\palette_tests-4dbbd8325f1633f4.exe)

running 6 tests
test t3_4_a_every_declared_palette_pair_meets_wcag_aa ... ok
test t3_4_d_no_tv_component_or_rendered_page_uses_a_hover_variant ... ok
test t3_4_b_the_tv_type_scale_is_at_most_six_sizes_and_never_under_twenty_eight_px ... ok
test t3_4_c_every_full_screen_tv_container_carries_the_overscan_class ... ok
test t3_4_a_every_pair_the_kiosk_actually_paints_is_in_the_table_and_passes_aa ... ok
test t3_4_a_no_source_file_on_either_surface_names_a_colour_outside_the_palette ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
```

### Transcript — photo_tests
```
cargo test --features server --test photo_tests

     Running tests\photo_tests.rs (target\debug\deps\photo_tests-0737157d86a8c225.exe)

running 7 tests
test t2_5_g_an_upload_without_a_parent_session_is_401_and_writes_nothing ... ok
test t2_5_c_an_svg_is_rejected_and_nothing_is_written ... ok
test t2_5_b_without_the_raised_limit_a_large_upload_413s ... ok
test t2_5_d_a_png_renamed_jpg_is_reencoded_with_the_correct_extension ... ok
test t2_5_e_uploads_are_served_with_nosniff_and_attachment ... ok
test t2_5_f_a_task_due_yesterday_is_hidden_and_delete_removes_row_and_file ... ok
test t2_5_a_a_real_12mp_photo_uploads_fast_and_small ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.20s
```

### Transcript — profiles_tests
```
cargo test --features server --test profiles_tests

     Running tests\profiles_tests.rs (target\debug\deps\profiles_tests-482727d4d549ebec.exe)

running 6 tests
test privileged_fn_without_a_session_errors ... ok
test a_fifth_and_sixth_profile_can_be_created ... ok
test setting_the_initial_pin_requires_the_real_setup_code ... ok
test rename_profile_persists_and_broadcasts_profiles_updated ... ok
test eight_parallel_wrong_pins_are_serialised_not_just_individually_delayed ... ok
test pin_verify_succeeds_once_and_backs_off_over_ten_failures ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 31.45s
```

### Transcript — pwa_tests
```
cargo test --features server --test pwa_tests

     Running tests\pwa_tests.rs (target\debug\deps\pwa_tests-6b82c9b3f0ab46f0.exe)

running 16 tests
test a_failed_send_stops_the_replay_and_keeps_the_remainder_in_order ... ok
test replay_sends_every_entry_once_and_a_second_replay_changes_nothing ... ok
test an_entry_older_than_forty_eight_hours_is_dropped_with_a_toast ... ok
test the_expiry_boundary_is_exactly_forty_eight_hours ... ok
test start_url_is_inside_scope_and_neither_path_carries_a_content_hash ... ok
test the_phone_has_the_five_bottom_tabs_the_plan_names ... ok
test the_pwa_doc_states_the_per_platform_offline_promise ... ok
test three_offline_mutations_queue_with_distinct_keys_and_their_own_dates ... ok
test the_queue_survives_a_serialisation_round_trip_with_keys_and_dates_intact ... ok
test no_source_file_routes_the_manifest_through_the_asset_pipeline ... ok
test the_served_service_worker_is_the_included_file ... ok
test service_worker_is_served_from_root_small_and_with_all_three_listeners ... ok
test manifest_is_served_from_root_with_the_fields_an_install_requires ... ok
test an_unknown_icon_name_is_a_404_and_not_a_file_read ... ok
test the_phone_page_links_the_manifest_at_its_root_url ... ok
test every_icon_the_manifest_lists_is_actually_served ... ok

test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s
```

### Transcript — realtime_tests
```
cargo test --features server --test realtime_tests

     Running tests\realtime_tests.rs (target\debug\deps\realtime_tests-eaed5d22bac78051.exe)

running 17 tests
test t1_2_1_backoff_matches_the_documented_schedule ... ok
test t1_2_8_the_midnight_tick_is_correct_across_both_dst_transitions ... ok
test t1_2_1_backoff_jitter_stays_within_twenty_percent ... ok
test t1_2_protocol_doc_names_every_message_variant ... ok
test t1_2_protocol_doc_states_the_normative_limits ... ok
test qa1_13_a_connected_client_receives_health_within_two_intervals ... ok
test qa1_10_an_invalid_stroke_is_dropped_without_closing_the_connection ... ok
test qa1_10_an_oversized_frame_closes_only_the_sender ... ok
test t1_2_2_a_lagging_client_is_resynced_and_the_socket_stays_open has been running for over 60 seconds
test t1_2_3_eight_clients_at_thirty_messages_per_second_for_thirty_seconds has been running for over 60 seconds
test t1_2_4_a_draw_is_echoed_to_both_clients_stamped_with_the_sender has been running for over 60 seconds
test t1_2_5_a_spoofed_server_message_reaches_nobody has been running for over 60 seconds
test t1_2_6_set_view_requires_a_parent_session has been running for over 60 seconds
test t1_2_7_a_client_reconnects_and_resnapshots_within_thirty_seconds has been running for over 60 seconds
test t1_2_9_a_flooding_client_is_throttled_and_closed_without_touching_the_others has been running for over 60 seconds
test t1_4_q1_11_a_cross_origin_websocket_upgrade_is_rejected has been running for over 60 seconds
test t1_4_q1_11_set_view_is_delivered_with_a_valid_session_cookie_and_no_bearer_auth has been running for over 60 seconds
test t1_2_2_a_lagging_client_is_resynced_and_the_socket_stays_open ... ok
test t1_2_4_a_draw_is_echoed_to_both_clients_stamped_with_the_sender ... ok
test t1_2_3_eight_clients_at_thirty_messages_per_second_for_thirty_seconds ... ok
test t1_2_5_a_spoofed_server_message_reaches_nobody ... ok
test t1_2_6_set_view_requires_a_parent_session ... ok
test t1_2_9_a_flooding_client_is_throttled_and_closed_without_touching_the_others ... ok
test t1_2_7_a_client_reconnects_and_resnapshots_within_thirty_seconds ... ok
test t1_4_q1_11_a_cross_origin_websocket_upgrade_is_rejected ... ok
test t1_4_q1_11_set_view_is_delivered_with_a_valid_session_cookie_and_no_bearer_auth ... ok

test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 132.00s
```

### Transcript — router_tests
```
cargo test --features server --test router_tests

     Running tests\router_tests.rs (target\debug\deps\router_tests-4204aa44f58ddb04.exe)

running 12 tests
test main_rs_is_under_twenty_five_lines_and_defines_no_routes ... ok
test root_redirects_permanently_to_tv ... ok
test tailwind_css_is_served_from_the_binary_at_a_stable_url ... ok
test manifest_stub_returns_manifest_json_content_type ... ok
test service_worker_stub_returns_200 ... ok
test screensaver_route_serves_a_jpeg_with_the_right_content_type ... ok
test uploads_route_serves_a_static_file ... ok
test m_route_serves_the_phone_routine_view ... ok
test tv_route_serves_the_kiosk_dashboard ... ok
test ca_cert_stub_returns_200 ... ok
test health_stub_returns_200 ... ok
test login_sets_a_well_formed_session_cookie ... ok

test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.94s
```

### Transcript — routine_tests
```
cargo test --features server --test routine_tests

     Running tests\routine_tests.rs (target\debug\deps\routine_tests-5e0d4ec5b8a04005.exe)

running 10 tests
test t1_5_pure_date_within_window_accepts_yesterday_today_and_tomorrow ... ok
test t1_5_pure_date_within_window_rejects_malformed_input ... ok
test t1_5_pure_date_within_window_rejects_three_days_either_way ... ok
test t1_5_claim_mutation_is_true_once_and_false_on_every_replay ... ok
test t1_5_2_toggling_three_days_ago_is_rejected_and_writes_nothing ... ok
test t1_5_4_a_profile_cannot_toggle_another_profiles_custom_task ... ok
test t1_5_5_toggle_custom_task_publishes_tasks_updated ... ok
test t1_5_3_the_same_idempotency_key_replayed_produces_one_row_change ... ok
test t1_5_q1_08_a_failed_fk_claim_releases_its_key_for_a_valid_users_replay ... ok
test t1_5_1_toggling_with_yesterdays_date_writes_yesterdays_row ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.24s
```

### Transcript — screensaver_tests
```
cargo test --features server --test screensaver_tests

     Running tests\screensaver_tests.rs (target\debug\deps\screensaver_tests-76ffd12a39bf65df.exe)

running 5 tests
test screensaver_images_are_served_with_nosniff_and_attachment ... ok
test an_upload_without_a_parent_session_is_401_and_writes_nothing ... ok
test a_non_image_payload_is_rejected_and_nothing_is_added ... ok
test screensaver_lists_at_least_three_placeholder_images_each_serving_as_jpeg ... ok
test uploading_a_new_image_makes_it_appear_in_the_list ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.10s
```

### Transcript — service_tests
```
cargo test --features server --test service_tests

     Running tests\service_tests.rs (target\debug\deps\service_tests-662ecdd70dbf2c9c.exe)

running 3 tests
test run_with_cwd_forced_to_system32_never_creates_a_db_there ... ok
test a_startup_bind_failure_is_logged_within_five_seconds ... ok
test run_generates_the_first_run_setup_code_and_logs_it_once_health_answers ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.03s
```

### Transcript — storage_tests
```
cargo test --features server --test storage_tests

     Running tests\storage_tests.rs (target\debug\deps\storage_tests-3239d896215e4b2d.exe)

running 10 tests
test generate_v1_fixture ... ignored, regenerates a committed binary fixture; run explicitly
test ambiguous_local_time_resolves_to_the_earliest_offset ... ok
test next_local_midnight_is_correct_across_both_dst_boundaries ... ok
test settings_round_trip_and_overwrite ... ok
test strokes_are_ordered_by_seq_and_cleared_by_the_watermark ... ok
test fresh_database_runs_every_embedded_migration ... ok
test twenty_concurrent_writers_see_no_sqlite_busy ... ok
test v1_database_is_baselined_and_every_log_row_survives ... ok
test vacuum_into_backup_restores_to_identical_row_counts ... ok
test pragmas_are_wal_normal_and_thirty_second_busy_timeout ... ok

test result: ok. 9 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.98s
```

### Transcript — tls_tests
```
cargo test --features server --test tls_tests

     Running tests\tls_tests.rs (target\debug\deps\tls_tests-e1e42a3d46001d81.exe)

running 7 tests
test tls_d_leaf_is_397_days_and_covers_every_non_loopback_host_ipv4 ... ok
test tls_b_ca_crt_is_served_over_plain_http_and_parses_as_a_certificate_authority ... ok
test tls_c_http_origin_redirects_the_phone_surface_and_serves_the_tv ... ok
test tls_a_rustls_client_with_the_local_ca_gets_health_200_over_https ... ok
test tls_g_the_join_qr_svg_decodes_to_the_https_phone_url ... ok
test tls_e_a_leaf_with_29_days_left_is_reissued_and_served_without_a_restart ... ok
test tls_f_mdns_answers_an_a_query_for_familyhub_local_with_this_hosts_ip ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.61s
```

### Transcript — tv_tests
```
cargo test --features server --test tv_tests

     Running tests\tv_tests.rs (target\debug\deps\tv_tests-9abd642d94eaed83.exe)

running 22 tests
test t2_1_d_every_key_in_the_d8_map_has_a_defined_transition ... ok
test t2_1_e_a_child_completes_the_whole_routine_with_the_remote_alone ... ok
test t2_1_a_the_focus_order_does_not_depend_on_how_often_it_is_rendered ... ok
test t2_1_b_an_injected_set_view_changes_the_rendered_view ... ok
test t2_1_c_an_injected_set_active_profile_changes_the_rendered_profile ... ok
test t2_1_a_exactly_one_element_wears_the_live_ring_and_it_is_the_focused_one ... ok
test t2_1_a_every_focusable_element_carries_a_visible_focus_ring ... ok
test t2_1_f_every_full_screen_container_carries_the_five_percent_overscan ... ok
test the_join_qr_overlay_still_renders_before_the_hub_knows_its_own_address ... ok
test t2_1_f_the_kiosk_has_no_hover_only_affordance ... ok
test t2_1_d_there_is_no_escape_key_anywhere_in_the_kiosk ... ok
test t2_1_b_set_view_reaches_every_panel_including_a_phones_restore ... ok
test t2_1_f_every_rendered_font_size_is_on_the_committed_allowlist ... ok
test the_kiosk_badge_keeps_the_servers_ninety_second_semantics ... ok
test t2_4_e_a_failed_calendar_fetch_is_not_rendered_as_an_empty_day ... ok
test t2_1_e_every_routine_item_is_within_twelve_presses_of_the_profile_selector ... ok
test the_join_qr_overlay_shows_the_https_phone_url_and_a_scannable_code ... ok
test the_kiosk_never_reaches_for_a_pointer_event ... ok
test t2_1_f_every_heading_clears_forty_four_pixels ... ok
test t2_1_a_the_rendered_focus_order_matches_the_golden_file ... ok
test the_key_code_debug_overlay_is_off_unless_keys_equals_one ... ok
test the_updated_line_is_permanent_and_the_badge_is_not ... ok

test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```

### Transcript — whiteboard_tests
```
cargo test --features server --test whiteboard_tests

     Running tests\whiteboard_tests.rs (target\debug\deps\whiteboard_tests-375cdf2901cdf8d0.exe)

running 3 tests
test t2_3_a_five_hundred_strokes_persist_and_replay_in_seq_order ... ok
test t2_3_b_clear_moves_the_watermark_then_compaction_removes_the_rows ... ok
test t2_3_c_undo_removes_only_the_callers_own_last_stroke ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 17.19s
```

### Transcript — docs_tests
```
cargo test --features server --test docs_tests

     Running tests\docs_tests.rs (target\debug\deps\docs_tests-76ef831c94706cb2.exe)

running 17 tests
test fire_tv_doc_documents_all_three_branches ... ok
test fire_tv_doc_exists_with_status_line ... ok
test owner_checklist_has_a_device_row ... ok
test t3_2_fire_tv_covers_every_required_string ... ok
test t3_2_owner_checklist_has_eight_numbered_steps_each_with_a_pass_criterion ... ok
test t3_2_recovery_covers_at_least_four_named_failure_modes ... ok
test t3_2_every_runbook_doc_exists_and_is_substantial ... ok
test test_dev_windows_md_exists_with_path_prefix ... ok
test t3_2_the_runbooks_cross_reference_each_other ... ok
test test_non_rust_md_exists_with_required_content ... ok
test test_screensaver_assets_exist ... ok
test test_tailwind_config_no_index_html ... ok
test test_photo_fixture_has_sufficient_resolution ... ok
test t3_3_every_task_id_appears_exactly_once_in_verification ... ok
test test_pwa_icons_are_generated_with_correct_dimensions ... ok
test t3_2_every_internal_doc_link_resolves ... ok
test test_maskable_icons_have_ten_percent_safe_zone_padding ... ok

test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
```

### Transcript — lib (unit tests, `src/**`)
```
cargo test --features server --lib

     Running unittests src\lib.rs (target\debug\deps\family_calendar-29665c907571d89c.exe)

running 198 tests
test client::components::calendar::tests::a_week_with_no_events_at_all_is_empty_and_one_event_is_ready ... ok
test client::components::calendar::tests::only_local_events_expose_a_row_id_to_delete ... ok
test client::components::calendar::tests::the_four_states_are_distinct_and_an_empty_answer_is_empty ... ok
test client::components::calendar::tests::the_window_label_slices_local_time_and_never_converts_it ... ok
test client::components::mobile::pwa::tests::every_manifest_icon_resolves_to_an_embedded_file ... ok
test client::components::mobile::pwa::tests::scope_containment_matches_the_manifest_algorithm ... ok
test client::components::mobile::pwa::tests::the_hash_heuristic_catches_asset_pipeline_urls_and_nothing_else ... ok
test client::components::mobile::pwa::tests::the_manifest_is_valid_json_with_a_root_scope_and_a_phone_start_url ... ok
test client::components::mobile::pwa::tests::the_service_worker_is_small_enough_to_ship_inline ... ok
test client::components::mobile::queue::tests::a_corrupt_payload_yields_an_empty_queue_rather_than_a_panic ... ok
test client::components::mobile::queue::tests::a_queue_round_trips_through_json ... ok
test client::components::mobile::session::tests::a_body_value_that_is_not_six_plain_digits_is_never_interpolated ... ok
test client::components::mobile::queue::tests::the_queue_never_grows_past_its_cap ... ok
test client::components::mobile::session::tests::is_parent_is_false_with_no_shell_above_it ... ok
test client::components::mobile::session::tests::only_parent_authorises_the_parent_only_affordances ... ok
test client::components::mobile::settings::tests::only_six_ascii_digits_are_worth_sending ... ok
test client::components::mobile::session::tests::the_non_wasm_stub_never_claims_a_session ... ok
test client::components::mobile::settings::tests::the_first_run_state_renders_the_setup_form ... ok
test client::components::mobile::tests::every_tab_has_a_distinct_slug ... ok
test client::components::mobile::tests::the_bar_has_the_five_tabs_the_plan_names_in_order ... ok
test client::components::mobile::storage::tests::a_value_round_trips_and_can_be_removed ... ok
test client::components::mobile::settings::tests::the_parent_state_renders_sign_out ... ok
test client::components::mobile::tests::the_default_tab_is_the_routine ... ok
test client::components::palette::tests::a_colour_utility_splits_into_its_token_and_opacity ... ok
test client::components::palette::tests::a_profile_colour_that_is_not_a_hex_triple_is_rejected ... ok
test client::components::palette::tests::compositing_matches_what_the_browser_paints ... ok
test client::components::palette::tests::every_pair_names_tokens_that_are_in_the_palette ... ok
test client::components::mobile::settings::tests::the_signed_out_state_renders_the_pin_form ... ok
test client::components::palette::tests::every_palette_pair_clears_the_stricter_body_floor_too ... ok
test client::components::palette::tests::every_palette_pair_meets_wcag_aa_for_its_size ... ok
test client::components::palette::tests::every_seeded_profile_colour_gets_an_ink_that_passes_aa ... ok
test client::components::palette::tests::print_the_contrast_table ... ok
test client::components::palette::tests::the_contrast_formula_reproduces_the_wcag_reference_values ... ok
test client::components::palette::tests::the_focus_indicator_meets_the_non_text_floor_on_both_of_its_edges ... ok
test client::components::palette::tests::the_pair_table_has_no_duplicates ... ok
test client::components::qr::tests::the_join_url_is_the_https_phone_origin ... ok
test client::components::routine::tests::a_failed_fetch_with_no_bus_value_is_an_explicit_error_not_a_default ... ok
test client::components::routine::tests::bus_today_wins_even_over_a_successful_fetch ... ok
test client::components::routine::tests::no_bus_value_falls_back_to_the_fetch_outcome ... ok
test client::components::qr::tests::the_rendered_svg_carries_explicit_dimensions ... ok
test client::components::routine::tests::nothing_has_answered_yet_is_loading_not_error ... ok
test client::components::routine::tests::ready_state_exposes_its_date_for_a_mutation ... ok
test client::components::routine::tests::idempotency_keys_are_never_repeated_on_this_client ... ok
test client::components::screensaver::idle_tracker_tests::a_single_tick_past_the_timeout_still_fires ... ok
test client::components::qr::tests::encoding_is_deterministic ... ok
test client::components::screensaver::idle_tracker_tests::activity_resets_the_idle_clock ... ok
test client::components::palette::tests::worst_case_ink_still_clears_the_large_text_floor ... ok
test client::components::screensaver::idle_tracker_tests::idle_tracker_fires_at_exactly_600_seconds ... ok
test client::components::screensaver::view_after_activity_tests::activity_clears_a_scheduled_overlay ... ok
test client::components::screensaver::view_after_activity_tests::activity_leaves_every_other_view_untouched ... ok
test client::components::tv::keymap::tests::a_logged_press_records_whether_the_kiosk_acted_on_it ... ok
test client::components::tv::keymap::tests::escape_is_not_in_the_map ... ok
test client::components::tv::keymap::tests::only_keys_equals_one_turns_the_debug_overlay_on ... ok
test client::components::tv::keymap::tests::the_back_button_is_accepted_under_all_three_spec_names ... ok
test client::components::tv::keymap::tests::the_key_log_is_bounded_and_keeps_the_newest_presses ... ok
test client::components::tv::keymap::tests::the_seven_remote_keys_map ... ok
test client::components::tv::keymap::tests::unknown_keys_fall_through_rather_than_guessing ... ok
test client::components::tv::model::tests::a_phones_restore_puts_the_television_back_on_the_routine ... ok
test client::components::tv::model::tests::an_open_overlay_owns_the_entire_focus_order ... ok
test client::components::tv::model::tests::dom_ids_are_stable_and_slugged ... ok
test client::components::tv::model::tests::panels_cycle_in_both_directions_and_wrap ... ok
test client::components::tv::model::tests::set_active_profile_for_an_unknown_id_changes_nothing ... ok
test client::components::tv::model::tests::set_active_profile_moves_the_rail_cursor_onto_that_profile ... ok
test client::components::tv::model::tests::switching_to_a_shorter_panel_clamps_the_body_cursor ... ok
test client::components::tv::model::tests::switching_to_an_empty_panel_returns_the_cursor_to_the_rail ... ok
test client::components::tv::model::tests::the_rail_always_ends_with_the_phone_qr ... ok
test client::components::tv::model::tests::the_whiteboard_panel_has_nothing_to_focus_because_drawing_is_phone_only ... ok
test client::components::tv::nav::tests::a_child_can_complete_the_whole_routine_with_the_remote_alone ... ok
test client::components::tv::nav::tests::an_open_overlay_swallows_navigation_keys ... ok
test client::components::tv::nav::tests::backspace_leaves_the_list_and_then_returns_to_the_routine_panel ... ok
test client::components::tv::nav::tests::down_in_the_list_walks_the_routine_and_wraps ... ok
test client::components::tv::nav::tests::down_on_the_rail_switches_to_the_next_profile ... ok
test client::components::tv::nav::tests::enter_in_the_list_toggles_the_focused_item ... ok
test client::components::tv::nav::tests::enter_on_a_profile_steps_into_that_profiles_routine_list ... ok
test client::components::tv::nav::tests::enter_on_the_rails_qr_entry_opens_the_overlay ... ok
test client::components::tv::nav::tests::enter_on_the_whiteboard_panel_cannot_strand_the_cursor_in_an_empty_list ... ok
test client::components::tv::nav::tests::play_pause_opens_the_phone_qr_and_any_dismiss_key_closes_it ... ok
test client::components::tv::nav::tests::right_and_left_cycle_the_panels ... ok
test client::components::tv::nav::tests::up_on_the_rail_wraps_to_the_qr_entry_and_selects_no_profile ... ok
test client::components::tv::shell::tests::the_bus_date_beats_the_clock_poll_and_absence_is_not_a_default ... ok
test client::components::tv::shell::tests::the_fallback_rail_is_the_four_seeded_children ... ok
test client::components::tv::staleness::tests::a_dropped_socket_lights_the_badge_without_waiting_for_the_threshold ... ok
test client::components::tv::staleness::tests::each_message_restarts_the_whole_window ... ok
test client::components::tv::staleness::tests::the_badge_clears_within_two_seconds_of_the_hub_answering ... ok
test client::components::tv::staleness::tests::the_badge_lights_past_ninety_seconds_of_silence ... ok
test client::components::tv::staleness::tests::the_badge_stays_off_through_ninety_seconds_of_silence ... ok
test client::components::tv::staleness::tests::the_status_line_is_permanent_even_before_the_first_answer ... ok
test client::components::tv::style::tests::every_type_scale_entry_clears_the_ten_foot_minimum ... ok
test client::components::tv::style::tests::the_focused_element_is_the_only_one_wearing_the_live_ring ... ok
test client::components::tv::style::tests::the_heading_sizes_clear_the_heading_minimum ... ok
test client::components::whiteboard::tests::fifty_queued_draws_between_two_render_ticks_are_all_drained ... ok
test client::components::whiteboard::tests::resize_triggers_a_repaint_from_the_stroke_log ... ok
test client::realtime::tests::a_stroke_expands_into_pairwise_segments ... ok
test client::realtime::tests::backoff_base_follows_the_documented_schedule_and_caps_at_thirty ... ok
test client::realtime::tests::backoff_stays_within_twenty_percent_of_the_base ... ok
test client::realtime::tests::echo_suppression_only_skips_our_own_origin ... ok
test client::realtime::tests::stroke_batcher_anchors_each_flush_to_the_previous_one ... ok
test client::realtime::tests::stroke_batcher_simplifies_points_closer_than_the_threshold ... ok
test client::realtime::tests::stroke_batcher_emits_at_most_thirty_messages_per_second ... ok
test server::api::realtime::tests::outbound_queue_collapses_into_one_resync_after_32_drops ... ok
test server::api::realtime::tests::outbound_queue_drops_the_oldest_frame_at_capacity ... ok
test server::api::realtime::tests::rate_limiter_never_resyncs_a_client_inside_its_budget ... ok
test server::api::realtime::tests::rate_limiter_resyncs_after_three_consecutive_over_budget_seconds ... ok
test server::api::realtime::tests::the_health_heartbeat_interval_is_inside_the_staleness_threshold ... ok
test client::components::tv::nav::tests::every_routine_item_is_within_twelve_presses_of_a_booted_kiosk ... ok
test server::api::realtime::tests::token_bucket_allows_the_burst_then_refills_at_the_configured_rate ... ok
test server::api::realtime::tests::unknown_client_json_does_not_parse_as_a_client_message ... ok
test server::api::realtime::tests::valid_stroke_accepts_what_the_real_client_sends ... ok
test server::api::realtime::tests::valid_stroke_rejects_a_nan_infinite_or_out_of_range_width ... ok
test server::api::realtime::tests::valid_stroke_rejects_an_empty_or_oversized_point_list ... ok
test server::api::realtime::tests::valid_stroke_rejects_an_unbounded_or_non_hex_color ... ok
test server::api::realtime::tests::valid_stroke_rejects_points_outside_the_normalised_unit_square ... ok
test server::api::screensaver::schedule_tests::disabled_schedule_never_emits_at_any_hour ... ok
test server::api::screensaver::schedule_tests::enabled_schedule_does_not_repeat_within_the_same_hour ... ok
test server::api::screensaver::schedule_tests::enabled_schedule_emits_only_at_its_configured_hour ... ok
test server::api::screensaver::schedule_tests::from_config_hour_none_matches_default ... ok
test server::api::screensaver::schedule_tests::from_config_hour_some_enables_at_that_hour ... ok
test server::api::screensaver::schedule_tests::schedule_is_disabled_by_default ... ok
test server::auth::tests::backoff_delay_is_capped_and_starts_at_one_millisecond ... ok
test server::auth::tests::backoff_delay_is_monotonically_increasing_and_at_least_two_to_the_n_ms ... ok
test server::auth::tests::constant_time_eq_matches_exact_strings_only ... ok
test server::auth::tests::pin_format_accepts_exactly_six_digits ... ok
test server::auth::tests::require_session_rejects_empty_and_unknown_tokens ... ok
test server::api::realtime::tests::the_websocket_message_cap_leaves_room_for_a_full_stroke ... ok
test server::auth::tests::same_origin_or_absent_rejects_cross_origin ... ok
test server::auth::tests::same_origin_or_absent_allows_no_origin_and_matching_origin ... ok
test server::auth::tests::session_from_headers_finds_the_cookie_among_others ... ok
test server::auth::tests::session_store_issue_revoke_and_expiry ... ok
test server::api::tv::tests::the_hub_reports_its_own_local_time_in_both_formats ... ok
test server::backup::tests::backup_file_name_has_minute_resolution_and_sorts_chronologically ... ok
test server::backup::tests::cutoff_thirty_days_back_is_thirty_days_earlier ... ok
test server::backup::tests::rotated_log_path_appends_a_dotted_generation ... ok
test server::backup::tests::uploads_snapshot_dir_matches_the_db_file_stem ... ok
test server::calendar::tests::a_week_starts_on_sunday_and_has_seven_calendar_days ... ok
test server::calendar::tests::an_all_day_occurrence_renders_as_a_date_and_a_timed_one_as_rfc3339 ... ok
test server::calendar::tests::rfc3339_local_never_relabels_local_time_as_utc ... ok
test server::calendar::tests::the_us_dst_week_still_has_seven_days_and_no_repeats ... ok
test server::calendar::tests::timestamps_round_trip_through_the_stored_format ... ok
test server::config::tests::default_http_addr_is_zero_zero_zero_zero_eight_zero_eight_zero ... ok
test server::config::tests::env_addr_overrides_default ... ok
test server::calendar::tests::an_empty_or_unparsable_rule_is_an_error_not_a_panic ... ok
test server::calendar::tests::a_draft_with_a_bad_rule_is_rejected_before_it_is_stored ... ok
test server::config::tests::env_screensaver_schedule_hour_overrides_file_and_default ... ok
test server::config::tests::env_data_dir_overrides_file_and_default ... ok
test server::config::tests::every_path_is_absolute_under_data_dir ... ok
test server::config::tests::file_data_dir_is_used_when_env_is_unset ... ok
test server::config::tests::file_screensaver_schedule_hour_is_used_when_env_is_unset ... ok
test server::backup::tests::rotate_log_if_needed_is_a_no_op_when_the_file_is_missing ... ok
test server::config::tests::screensaver_schedule_hour_defaults_to_none ... ok
test server::config::tests::toml_parser_reads_flat_and_sectioned_keys ... ok
test server::health::tests::a_fresh_message_resets_the_ninety_second_clock ... ok
test server::health::tests::badge_stays_off_for_ninety_seconds_of_silence ... ok
test server::health::tests::badge_turns_off_within_two_seconds_of_a_message ... ok
test server::health::tests::badge_turns_on_past_ninety_seconds_of_silence ... ok
test server::health::tests::last_google_poll_round_trips_through_the_recorder ... ok
test server::mdns::tests::both_service_types_are_dns_sd_shaped ... ok
test server::mdns::tests::the_advertised_hostname_is_an_fqdn_with_a_trailing_dot ... ok
test server::health::tests::rfc3339_formats_a_known_instant ... ok
test server::pki::tests::cert_source_defaults_to_self_signed_and_rejects_unknown_modes ... ok
test server::router::tests::only_the_phone_surface_is_upgraded_to_https ... ok
test server::router::tests::ensure_public_dir_exists_creates_the_directory ... ok
test server::mdns::tests::there_is_exactly_one_daemon_per_process ... ok
test server::service::tests::configure_firewall_names_every_rule_family_hub_prefixed ... ok
test server::service::tests::configure_power_plan_disables_standby_and_hibernate ... ok
test server::router::tests::public_bundle_present_is_false_when_the_directory_does_not_exist_at_all ... ok
test server::router::tests::the_upgrade_keeps_the_requested_host_and_query ... ok
test server::service::tests::dispatch_on_an_unknown_subcommand_returns_a_nonzero_exit_code_not_a_panic ... ok
test server::router::tests::public_bundle_present_is_false_for_an_empty_directory_and_true_with_a_wasm_file ... ok
test server::config::tests::ensure_dirs_and_log_creates_every_directory_and_logs_each_path ... ok
test server::service::tests::default_log_level_drops_debug_and_trace_but_keeps_info_and_above ... ok
test server::service::tests::install_configures_three_firewall_rules_and_the_power_plan ... ok
test server::service::tests::family_hub_log_env_var_raises_the_level_to_debug ... ok
test server::service::tests::install_refuses_when_no_wasm_bundle_is_present_beside_the_executable ... ok
test server::service::tests::install_still_reports_success_when_the_firewall_and_power_commands_fail ... ok
test server::service::tests::start_on_an_uninstalled_mock_is_a_clean_error_not_a_panic ... ok
test server::service::tests::install_registers_the_service_pointed_at_the_given_executable ... ok
test server::service::tests::install_succeeds_once_the_wasm_bundle_is_present_beside_the_executable ... ok
test server::service::tests::install_then_start_then_status_then_stop_then_uninstall_round_trips ... ok
test server::service::tests::uninstall_on_a_fresh_mock_reports_not_installed ... ok
test server::service::tests::service_logger_writes_events_to_the_log_file ... ok
test server::service::tests::install_with_forwards_the_real_running_executable ... ok
test server::service::tests::tv_probe_connects_when_adb_succeeds ... ok
test server::service::tests::tv_probe_reports_unreachable_rather_than_erroring_when_adb_fails ... ok
test server::service::tests::tv_probe_without_any_configured_ip_says_so_rather_than_panicking ... ok
test server::tls::tests::installing_the_ring_provider_twice_is_a_no_op ... ok
test server::tls::tests::pem_block_rejects_a_missing_label ... ok
test server::service::tests::warn_and_error_events_flush_immediately_without_an_explicit_flush_call ... ok
test client::components::tv::nav::tests::every_routine_item_is_within_twelve_presses_from_any_panel ... ok
test server::pki::tests::the_generated_ca_certificate_is_a_ca ... ok
test server::pki::tests::open_is_idempotent_and_keeps_the_same_ca ... ok
test server::pki::tests::a_freshly_issued_leaf_is_not_due_for_renewal ... ok
test server::router::tests::pki_for_returns_the_same_authority_for_the_same_directory ... ok
test server::pki::tests::reissue_replaces_the_leaf_in_place_and_on_disk ... ok
test server::tls::tests::a_leaf_pem_pair_becomes_a_rustls_certified_key ... ok
test server::tls::tests::the_resolver_hands_back_whatever_leaf_was_last_installed ... ok
test server::config::tests::out_of_range_screensaver_schedule_hour_panics_at_startup - should panic ... ok
test server::auth::tests::hash_and_verify_round_trip ... ok
test server::service::tests::writing_twenty_megabytes_of_log_lines_rotates_under_the_cap ... ok

test result: ok. 198 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.85s
```


**Note on pre-existing T2.3 residual:** superseded — see the QA round 1 section below.

## QA round 1 fix — Q1-09 (T2.3): ordered stroke persistence

`docs/qa/QA_ROUND_1.md` Q1-09 flagged `record_stroke`'s write-behind design: each
stroke was persisted from its own detached `tokio::spawn`'d task, so rows could
commit out of `seq` order and land after the broadcast, and the two recorded
flakes (`loop_tests` ~15%, `whiteboard_tests` 500-stroke count — T2.3 H-21) were
this race. `phase-qa1/T2.3` replaced it with a single ordered persistence task:
`record_stroke` mints `seq` synchronously and `send()`s a `PendingStroke` down an
unbounded channel; one task `recv()`s, drains `try_recv()` into a batch, and
inserts the batch in `seq` order (a single-row batch skips the `BEGIN`/`COMMIT`
wrapper and inserts directly, since ordering here comes from the one writer
processing its queue in order, not from the transaction). `snapshot` now
returns `latest` as the highest **contiguous** `seq` from `since_seq`, so a
bookmark can never claim coverage past a row still in the channel.

Getting the writer task's *lifetime* right took two attempts, both proven by
running `loop_tests`/`whiteboard_tests` 20× as Q1-09 asks:

1. **First attempt — `tokio::spawn`ed lazily onto the caller's runtime,
   cached in a `OnceLock`.** This is what the write-behind rows above
   describe structurally, but a lazily-`tokio::spawn`ed task is tied to
   *whichever* tokio runtime is live when it first starts, and each
   `#[tokio::test]` function owns its own runtime. The writer spawned during
   one test's run was silently killed the moment that test's runtime shut
   down; every later test in the same binary either sent its strokes into a
   dead channel and lost them outright, or — worse — raced the just-dying
   runtime's teardown and reused a sender whose receiver was mid-drop,
   losing *some* rows. Both shapes were reproduced live: a first `loop_tests`
   ×20 trial (16/20 green, 4 failed) and a second (18/20, 2 failed), then —
   after also making a same-runtime respawn-on-close variant — a full
   `cargo test --features server` run where all three `whiteboard_tests`
   failed together (`82/500` strokes persisted; a stray stroke bled into the
   undo test) once two tests' runtimes overlapped during teardown.
2. **Fix — the writer runs on its own dedicated OS thread with its own tiny
   `current_thread` runtime, created once via `OnceLock` and never torn
   down.** Nothing ever kills that runtime while the process is alive, so
   there is no teardown window to race and no dead channel to fall into,
   regardless of how many ephemeral tokio runtimes come and go around it
   (exactly `#[tokio::test]`'s pattern, and — by the same reasoning — the one
   real production runtime too).

Final counts, with the dedicated-thread writer, `cargo test --features server
--test <name> -- --test-threads=1`, one full process per run:

| Suite | Runs green | Runs failed |
| --- | --- | --- |
| `whiteboard_tests` (3 tests/run) | 20 / 20 | 0 |
| `loop_tests` (1 test/run) | 20 / 20 | 0 |

Both flakes Q1-09 named are confirmed gone: 20/20 clean for each suite,
including the 500-stroke seq-order replay and the phone→TV restart-and-resnapshot
test that used to fail intermittently. The full baseline
(`cargo test --features server`) was re-run after the dedicated-thread fix and
was green throughout (fmt clean; clippy clean on both `server` and
`web`/wasm32 targets).

**Re-verified for this T3.3 pass (Q3-03), replacing the paragraph above that
previously contradicted the 20/20 table with a claimed residual failure
rate:** `loop_tests`' single test
(`t2_6_phone_drives_the_tv_across_a_server_restart`) was run 10× in this
pass, one full process per run (`cargo test --features server --test
loop_tests`, each teed to its own `scratch/loop_run_<n>.log`):

| Run | Result |
| --- | --- |
| 1–10 | `ok. 1 passed; 0 failed` (6.10 s–6.15 s each) |

**10 / 10 green.** No failure of any kind was observed in this run of the
suite on this machine; the true count is 10/10, not the earlier
"residual failure rate … runs 8-11 and 9-12" claim, which is superseded by
this measurement. The full baseline (`cargo test --features server`) was
re-run after these changes and was green — see the Summary below and the
`## Transcripts` section above for the representative single-run transcript.

## Rendered in Chrome

**Date:** 2026-08-30 (Boss pass, after the T3.3 squash-merge to `main`)  
**Browser:** Chrome on the Windows 11 dev box, driven through the Claude-in-Chrome MCP tools (`tabs_create_mcp`, `navigate`, `resize_window`, `computer`, `read_console_messages`, `read_network_requests`). The tools were available; everything below was observed, not inferred.  
**Server:** release build started in the background with `FAMILY_HUB_DATA_DIR` pointed at a fresh temp directory (`family.db` migrated to version 3, PKI generated, `/health` answered `{"db":true,"ws_clients":…,"migration_version":3,"days_to_expiry":396}`). Two binaries were exercised — see the first finding.

### Finding 1 — the `cargo build --release --features server` binary serves an unstyled `/tv`

`target/release/family-calendar.exe` (plain cargo, no `dx`) SSRs this stylesheet tag on `/tv`:

```html
<link rel="stylesheet" href="/assets/This should be replaced by dx as part of the build process. If you see this error, make sure you are using a matching version of dx and dioxus and you are not stripping symbols from your binary."/>
```

That is the un-rewritten `asset!("/assets/tailwind.css")` placeholder from `src/client/app.rs`: manganis only rewrites it when the **server** binary is produced by `dx build`. The request 503s, no Tailwind CSS is applied, and the kiosk renders as browser-default HTML (Times New Roman, unstyled buttons, no visible focus ring) — screenshot: [tv-cargo-binary-unstyled.jpg](verification/tv-cargo-binary-unstyled.jpg). The wasm client still hydrated (server fns were called, the WebSocket connected) but the page is unusable on a television.

`dx build --platform web --release` builds the same server (`target/dx/family-calendar/release/web/server.exe`, with `public/` beside it) with the link rewritten to `/assets/tailwind-dxhe5dba96a1372f1ff.css` (200, `text/css`). **The shippable Windows binary is dx's `server.exe`, not `cargo build --release`'s `family-calendar.exe`.** CI's final "Windows-x64 release build" step (`cargo build --features server --release`) therefore produces an artefact that must not be installed as-is; `docs/DEV_WINDOWS.md` and `docs/FIRE_TV.md` should point the owner at the dx output. Carried to `docs/HANDOFF.md` for T3.5.

Everything below was captured against the dx-built `server.exe`.

### `/tv` at 1920×1080 (`http://127.0.0.1:8080/tv`)

Window resized to 1920×1080 via `resize_window` (the tool reports the capture at 1510×812 because the display is DPI-scaled; the layout is the 1920-wide one). Screenshot: [tv-1920x1080-routine.jpg](verification/tv-1920x1080-routine.jpg).

- Header: "Morning Routine · Boy 1" in the T3.4 blue, "updated HH:MM" top-right, **no red Disconnected badge**.
- Left rail: the four seeded profiles (Boy 1–4, red/amber/green/blue avatars) as large cards; Boy 1 selected with the yellow focus ring on it (autofocus on mount worked without a click).
- Right: progress bar plus the `0 / 8` pill, the eight seeded routine items as large checkbox cards with title and subtitle, scrollable.
- Bottom: the three panel tabs (Morning Routine / Today / Whiteboard), the current one filled.
- Overscan margin visible on all four sides; nothing clipped at the edges; type comfortably readable at 10-ft scale.

### Console errors

`read_console_messages` on `/tv` across two full page loads: **zero errors or warnings from the application.** The only three entries are `Error: A listener indicated an asynchronous response by returning true, but the message channel closed before a response was received` at `/tv:0:0`, which is emitted by an installed Chrome extension (the Acrobat extension `efaidnbmnnnibpcajpcglclefindmkaj`, visible in the same tab's network log injecting its content scripts) — not by the page.

Network on `/tv`: `GET /tv` 200, the hashed `.js` / `.wasm` / `.css` assets 200, server fns `tv_clock`, `list_profiles`, `get_daily_routine`, `get_custom_tasks`, `get_today_events`, `list_screensaver_images` all `POST … 200`. One oddity worth a note, not a fix: the page's `<link rel="manifest" href="/manifest.webmanifest">` on the HTTP origin answers 308 to `https://…:8443/manifest.webmanifest` (by design — the manifest is an `/m`-only route), which Chrome logs as a failed manifest fetch on the TV. Harmless on the kiosk (it never installs a PWA), but it is one extra request per load.

### WebSocket

**Connected.** `/health` reported `"ws_clients":1` while the single `/tv` tab was open (0 before it was opened), the header showed no Disconnected badge, and after the first server process was killed and the dx `server.exe` started in its place the tab re-connected on its own (`ws_clients` back to 1 within about 6 s) without a reload — the T1.2 reconnect path works in a real browser. (The MCP network log does not list WebSocket upgrades, so `/health` is the evidence.)

### Routine driven by keyboard (D-pad emulation)

Keys sent with `computer(action: "key")` to the freshly loaded page, no mouse click first:

| Press | Observed |
| --- | --- |
| `Enter` | focus ring moved from the Boy 1 rail card into the list — yellow ring on item 1 "Wake up and thank God for the day!" |
| `ArrowDown` | ring moved to item 2 "Make your bed" |
| `Enter` | `POST /api/toggle_routine_task` 200; item 2 rendered checked (filled blue tick), progress bar advanced, pill became `1 / 8` — screenshot: [tv-routine-after-enter-toggle.jpg](verification/tv-routine-after-enter-toggle.jpg) |
| `ArrowRight` | panel cycled to **Today** ("Nothing on the calendar today.", the Today tab filled) — screenshot: [tv-today-after-arrowright.jpg](verification/tv-today-after-arrowright.jpg) |

So the routine is fully drivable with arrows + Enter, and the focus ring is visible on every step. One caveat that is a harness artefact, not an app fault: when the key handler's `<div tabindex="0">` loses focus (one click on empty body, on the unstyled build) key presses go nowhere until the surface is focused again — the same as any remote-driven kiosk, and the shell's `onmounted` autofocus covers the real boot. Also: `computer(screenshot)` timed out for 30 s whenever the `/tv` tab sat in the background behind another tab (Chrome throttles background renderers); every capture taken with the tab in front succeeded, and the app itself never froze (`/health` and the server fns kept answering throughout).

### `/m` over HTTPS

`https://127.0.0.1:8443/m` hit Chrome's privacy interstitial for the hub's private CA (expected — the CA is not installed in this profile), and the MCP tools cannot attach to an interstitial (`Cannot attach to this target` / `Frame with ID 0 is showing error page`), so the bypass could not be clicked. The fallback `http://127.0.0.1:8080/m` is a 308 to the same HTTPS URL (T1.3, by design), so it lands on the same interstitial. **`/m` was therefore not rendered in Chrome in this pass.** Verified out-of-band instead: `curl -k https://127.0.0.1:8443/m` gives 200 with `<title>Sheffield Family Hub</title>` and the hashed Tailwind link, and `manifest.webmanifest` is 200 `application/manifest+json` on the TLS origin. Rendering `/m` on a phone with the CA installed is already a step of `docs/OWNER_CHECKLIST.md`; this Chrome pass does not replace it.

### Housekeeping

Background server stopped after the pass (`taskkill`), the temp data directory left under the session scratchpad, both Chrome tabs closed.

---

## HS wave — Homeschool ("School" tab), verified 2026-09-03

**Branch:** `hs/HS7`. This section is HS7's own verification pass — the seven rows
HS1–HS7 added to the **Results by Task** table above (HS2a/HS2b share the `HS2`
row, per `docs/homeschool/PLAN_HOMESCHOOL.md` §3 HS7; `HS8`, the fresh-context
QA loop, is excluded exactly as `T3.5` is excluded from the phase 0–3 pass
above). Every block below is pasted verbatim from a real, fresh run on this
machine — `cargo test --features server --test <name> 2>&1 | Tee-Object
scratch\full_run_3.log` (one process for the whole suite, teed once, matching
the T3.3 convention of one real invocation rather than per-binary re-runs) —
never reconstructed from source, and every `test result:` line below shows
`0 failed`.

### HS wave summary

**7 rows added: HS1–HS7, 0 FAIL.** The full baseline (`cargo test --features
server`) was run **twice consecutively** on this branch after every doc and
test change in it landed (`scratch/full_run_3.log`, `scratch/full_run_4.log`):

| Run | Binaries | Result |
| --- | --- | --- |
| 1 (`full_run_3.log`) | 32 `test result:` lines (30 integration binaries + `--lib` + doc-tests) | **0 failed**, every line |
| 2 (`full_run_4.log`) | same | **0 failed**, every line |

`cargo fmt --check`, `cargo clippy --features server --all-targets -- -D
warnings` and `cargo clippy --features web --target wasm32-unknown-unknown --
-D warnings` (the form `docs/HANDOFF.md` H-HS3-3 records — `--all-targets` on
the wasm target pulls dev-dependencies that do not compile for wasm at all,
pre-existing and unrelated to this wave) all exited 0. `dx build --platform
web --release` exited 0 (below).

**48 h vs ±1 day asymmetry (H-HS5-3, R-14):** the phone's offline queue keeps
a mutation for 48 hours, longer than the hub's own ±1 day `date` window —
`docs/PWA.md`'s "What works with the hub unreachable" section states this
plainly (a replay attempted after ~30 hours is accepted by the phone and
rejected by the hub, which then tells the owner which day it belonged to).
No open item; recorded here as the transcript row HS5's own handoff asked for.

**HS-10 (recorded in `docs/HANDOFF.md`, request to Boss/HS8):** `tests/curriculum_tests.rs`
— HS2b's own committed acceptance suite — is present on branch `hs/HS2b` but was
never squash-merged into `main`; it is therefore **not** part of this run's 32
binaries. HS2's row below is verified instead with the loader/`import-curriculum`
evidence real content produces, which is genuine but not a substitute for that
committed suite. See `docs/HANDOFF.md` "From HS7" for the full account and the
one-line fix requested.

### Transcript — homeschool_db_tests (HS1)

```
cargo test --features server --test homeschool_db_tests

     Running tests\homeschool_db_tests.rs (target\debug\deps\homeschool_db_tests-62742d09405844e4.exe)

running 26 tests
test add_extra_numbers_sort_order_within_the_profile_and_the_date ... ok
test a_fresh_database_migrates_to_version_five_with_foreign_keys_enforced ... ok
test a_file_with_one_bad_row_at_the_end_is_rejected_whole ... ok
test every_invalid_curriculum_file_is_rejected_by_name_and_line_and_writes_nothing ... ok
test an_extra_with_too_long_a_title_or_an_unknown_category_is_rejected ... ok
test enrolling_the_same_boy_twice_replaces_his_row_and_keeps_started_on ... ok
test extras_between_is_inclusive_on_both_ends_and_ordered_by_date_then_sort_order ... ok
test the_committed_fixture_is_the_shape_every_later_task_expects ... ok
test a_parent_edit_survives_a_reload_and_replace_restores_the_file_text ... ok
test deleting_a_profile_cascades_its_enrollment_its_log_and_its_extras ... ok
test loading_the_fixture_twice_leaves_the_row_counts_identical ... ok
test logs_and_log_counts_report_done_and_skipped_separately ... ok
test the_enrollment_seed_skips_a_renamed_profile_and_a_missing_curriculum ... ok
test load_and_seed_is_ok_even_when_every_file_in_the_directory_is_bad ... ok
test the_occurrence_key_dedupes_ticks_including_the_null_assignment_case ... ok
test a_bad_file_beside_a_good_one_loads_exactly_one_curriculum_and_logs_the_path ... ok
test set_subject_schedule_writes_days_and_shared_and_the_check_rejects_rubbish ... ok
test setting_and_clearing_an_extras_status_manages_its_completion_stamps ... ok
test the_enrollment_seed_enrolls_isaiah_once_and_a_second_boot_changes_nothing ... ok
test together_group_is_every_enrollment_sharing_a_curriculum_and_week ... ok
test update_extra_and_set_extra_status_both_bump_updated_at ... ok
test unenrolling_keeps_the_log_and_the_week_pointer_only_moves_when_told ... ok
test week_plan_returns_every_subject_but_only_this_weeks_rows ... ok
test replace_deletes_a_vanished_subject_and_counts_the_log_rows_it_took ... ok
test the_v1_fixture_migrates_to_version_five_and_keeps_every_routine_log_row ... ok
test no_curriculum_content_is_tracked_in_the_repository ... ok

test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.43s
```

### Transcript — HS2 (HS2a + HS2b transcription; `tests/curriculum_tests.rs` not on `main`, see HS-10 above)

**Boss, wave D close (2026-09-03):** HS-10 applied — `tests/curriculum_tests.rs`
re-applied from `hs/HS2b` (`657a588`) onto `main` as its own micro-commit and run
against the gitignored files present on this machine (its own N1 guard (h) walks
every tracked file, HS7's new docs included):

```
> cargo test --features server --test curriculum_tests
running 9 tests
test sha256_self_test::sha256_matches_the_well_known_empty_and_abc_vectors ... ok
test every_spot_check_row_matches_its_week_and_subject_by_contains ... ok
test the_chapter_sequence_subject_s_chapters_appear_once_each_in_non_decreasing_week_order ... ok
test the_two_ordinals_subject_has_two_rows_every_week_except_the_expect_file_s_exceptions ... ok
test term_note_counts_hold ... ok
test every_subject_s_days_are_within_the_five_day_school_week_and_shared_matches_its_category ... ok
test the_expect_file_s_weeks_subject_count_and_every_week_list_hold ... ok
test the_hs1_loader_accepts_the_file_as_both_a_parse_and_a_real_database_insert ... ok
test no_assignment_text_from_the_toml_appears_in_any_tracked_file ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.27s
```

The import-curriculum evidence HS7 gathered before the file landed follows unchanged.

The gitignored `curriculum/ao-year-1.toml` (N1) is present on this machine
(HS2a wave A + HS2b wave B). Verified with the shipped, real subcommand
against a fresh scratch data directory — no test file, no curriculum text in
any tracked file:

```
family-hub.exe import-curriculum <path to the gitignored ao-year-1.toml>

imported ao-year-1 from <scratch-dir>\curricula\ao-year-1.toml: 0 subjects, 0 assignments, 0 term notes inserted (existing rows left untouched)
```

("0 inserted" is correct, not a failure: opening the write pool for the first
time in this process already ran the boot-time loader over the same directory
— the file had copied in a step earlier — so every row existed before this
command's own `insert_missing` ran; H5's insert-missing-only contract held.)
Confirmed with `/health` (below) that the curriculum really loaded: `curricula: 1`.
`docs/homeschool/README.md`'s "Transcription status" note records both halves
done; the file's last `week = 36` row (checked directly, not quoted here per
N1) confirms HS2b's weeks 19–36 are present.

### Transcript — homeschool_tests (HS4)

```
cargo test --features server --test homeschool_tests

     Running tests\homeschool_tests.rs (target\debug\deps\homeschool_tests-b3cbdb54354866ad.exe)

running 16 tests
test hs4_j_get_homeschool_today_reports_nobody_enrolled_and_a_paused_boys_empty_lists ... ok
test hs4_i_set_subject_schedule_rejects_th_and_writes_nothing ... ok
test hs4_h_mark_all_done_ticks_only_unticked_items_and_is_idempotent ... ok
test hs4_g_set_school_week_reaches_year_complete_and_back_returns_to_the_last_week ... ok
test hs4_a_toggling_with_yesterdays_date_writes_yesterdays_row_and_three_days_ago_is_rejected ... ok
test hs4_f_toggle_lesson_together_writes_exactly_the_two_boys_sharing_the_week ... ok
test hs4_c_set_subject_schedule_without_a_session_errors_and_with_one_broadcasts_curriculum_updated ... ok
test hs4_b_the_same_idempotency_key_replayed_produces_one_row_change ... ok
test hs4_c_set_paused_without_a_session_errors_and_with_one_broadcasts_homeschool_updated ... ok
test hs4_d_toggle_lesson_succeeds_with_no_cookie_at_all ... ok
test hs4_e_an_occurrence_outside_the_current_week_and_an_unenrolled_boy_are_both_rejected ... ok
test hs4_l_get_month_fetches_the_current_week_plan_only_when_it_intersects_and_is_extras_only_when_unenrolled ... ok
test hs4_day_item_lesson_and_extra_are_distinguishable_on_a_real_today_view ... ok
test hs4_k_add_extra_requires_a_session_and_bounds_scheduled_date ... ok
test hs4_l_get_week_grid_bounds_and_datedness ... ok
test hs4_m_toggle_lesson_with_a_non_positive_subject_id_is_rejected_before_any_write ... ok

test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.72s
```

### Transcript — HS3 (shared scheduling core; sample from `--lib` and `realtime_tests`)

`shared::homeschool` carries no `chrono` (`grep -rn chrono src/shared/` — one
pre-existing doc-comment hit in `types.rs`, H-HS3-4) and compiles clean under
`cargo clippy --features web --target wasm32-unknown-unknown -- -D warnings`.
Trimmed to the `hs3_*`-prefixed unit tests inside the 296-test `--lib` run
(full block below):

```
test shared::homeschool::tests::hs3_a_weekday_reads_the_four_stated_dates_by_sakamotos_method ... ok
test shared::homeschool::tests::hs3_a_weekday_rejects_a_day_that_does_not_exist_because_2100_is_not_a_leap_year ... ok
test shared::homeschool::tests::hs3_a_add_days_crosses_month_year_and_leap_boundaries ... ok
test shared::homeschool::tests::hs3_b_a_daily_subject_with_no_rows_deals_one_untitled_occurrence_a_school_day ... ok
test shared::homeschool::tests::hs3_b_a_free_read_subject_is_never_an_occurrence ... ok
test shared::homeschool::tests::hs3_b_a_weekly_subject_deals_one_occurrence_on_the_first_of_its_days ... ok
test shared::homeschool::tests::hs3_b_rule_five_splits_one_row_over_two_days_into_part_one_and_part_two ... ok
test shared::homeschool::tests::hs3_b_rule_five_gives_two_rows_over_two_days_one_each_with_no_part_label ... ok
test shared::homeschool::tests::hs3_b_rule_five_puts_two_rows_over_one_day_both_on_that_day ... ok
test shared::homeschool::tests::hs3_b_rule_five_gives_the_earlier_group_the_extra_day_for_two_rows_over_three ... ok
test shared::homeschool::tests::hs3_b_rule_five_deals_nothing_when_the_rows_or_the_days_are_empty ... ok
test shared::homeschool::tests::hs3_b_a_monday_reading_unticked_on_wednesday_is_in_catch_up_and_not_due_today ... ok
test shared::homeschool::tests::hs3_b_a_daily_occurrence_reaches_catch_up_on_a_later_day_and_never_twice_in_due_today ... ok
test shared::homeschool::tests::hs3_b_a_saturday_is_not_a_school_day_and_leaves_everything_unfinished_in_catch_up ... ok
test shared::homeschool::tests::hs3_b_a_skipped_row_leaves_catch_up_and_counts_in_skipped_count ... ok
test shared::homeschool::tests::hs3_b_a_paused_enrollment_empties_every_list_without_touching_the_log ... ok
test shared::homeschool::tests::hs3_b_an_assignment_row_with_its_own_days_overrides_the_subjects ... ok
test shared::homeschool::tests::hs3_b_together_renders_a_shared_occurrence_once_and_names_the_boys_who_finished_it ... ok
test shared::homeschool::tests::hs3_b_a_year_complete_pointer_deals_nothing_and_is_not_an_error ... ok
test shared::homeschool::tests::hs3_b_finish_week_is_offered_once_the_week_is_complete_or_the_last_school_day_arrives ... ok
test shared::homeschool::tests::hs3_c_a_week_started_on_a_wednesday_puts_monday_and_tuesday_in_the_following_week ... ok
test shared::homeschool::tests::hs3_c_last_school_day_is_the_last_date_of_the_span_in_school_days ... ok
test shared::homeschool::tests::hs3_c_week_span_is_the_seven_days_from_the_anchor_and_term_follows_default_four ... ok
test shared::homeschool::tests::hs3_f_parse_days_rejects_an_unknown_letter_or_a_repeat ... ok
test shared::homeschool::tests::hs3_f_parse_days_returns_the_days_in_mtwrfsu_order_however_they_were_written ... ok
test shared::homeschool::tests::hs3_f_a_day_error_says_which_letter_was_wrong ... ok
test shared::homeschool::tests::hs3_g_the_week_grid_for_fixture_week_two_has_six_rows_of_five_cells ... ok
test shared::homeschool::tests::hs3_g_an_undated_grid_reports_dated_false_and_its_dates_are_only_advisory ... ok
test shared::homeschool::tests::hs3_h_september_2026_has_thirty_days_and_only_the_span_is_dealt_out ... ok
test shared::homeschool::tests::hs3_h_february_is_twenty_eight_days_in_2026_and_twenty_nine_in_2024 ... ok
test shared::homeschool::tests::hs3_h_with_nobody_enrolled_a_month_is_extras_only ... ok
test shared::homeschool::tests::hs3_i_merge_extras_files_each_task_by_its_date_and_counts_only_the_current_span ... ok
test shared::homeschool::tests::hs3_i_merge_extras_ignores_another_boys_task_and_counts_a_skipped_one ... ok
test shared::homeschool::tests::hs3_i_extras_join_a_boys_real_today_lists_alongside_his_lessons ... ok
```

Plus the protocol suite's own HS3 assertion:

```
cargo test --features server --test realtime_tests

test hs3_the_server_message_sample_vector_gained_exactly_the_two_homeschool_variants ... ok
...
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 131.41s
```

### Transcript — HS5 (phone School tab; sample from `glyph_tests` and `pwa_tests`)

```
cargo test --features server --test glyph_tests

     Running tests\glyph_tests.rs (target\debug\deps\glyph_tests-5a8afbe936675f44.exe)

running 23 tests
test d4_2_a_an_unknown_icon_name_falls_back_to_the_check ... ok
test d4_2_a_every_seeded_icon_name_maps_to_a_non_ascii_glyph ... ok
test hs5_a_the_widest_tab_label_fits_a_column_of_the_narrowest_phone ... ok
test d4_2_b_the_mobile_routine_row_renders_the_sun_glyph ... ok
test d4_2_b_the_mobile_routine_row_never_prints_the_raw_icon_name_again ... ok
test hs5_a_the_bar_renders_six_buttons_in_the_order_h6_names ... ok
test hs5_c_nobody_enrolled_offers_the_way_in_rather_than_a_blank_tab ... ok
test hs5_e_the_remote_offers_school_and_sends_the_view_the_kiosk_expects ... ok
test hs5_d_both_school_mutations_name_their_boy_and_their_label ... ok
test d4_2_c_the_screensaver_caption_names_the_hub_on_a_solid_dark_chip ... ok
test d4_2_c_an_inactive_screensaver_renders_nothing ... ok
test hs5_d_a_failed_extra_tick_is_queued_and_replayed_once ... ok
test hs5_d_a_failed_catch_up_lesson_tick_is_queued_and_replayed_once ... ok
test hs5_j_a_future_day_says_it_has_not_been_dealt_out_and_shows_extras_only ... ok
test hs5_h_the_year_view_lays_the_fixture_week_out_as_a_subject_by_day_grid ... ok
test hs5_j_only_a_parent_is_offered_the_add_task_form ... ok
test hs5_i_the_month_view_counts_only_what_it_can_honestly_count ... ok
test hs5_c_a_paused_group_says_school_is_out_and_a_finished_year_celebrates ... ok
test hs5_b_the_identical_render_as_a_parent_gains_exactly_the_parent_affordances ... ok
test hs5_b_the_catch_up_chip_names_the_day_it_slipped_from ... ok
test hs5_b_today_renders_the_fixture_the_way_h6_lays_it_out ... ok
test hs5_k_a_week_that_has_not_been_dealt_out_is_neither_dated_nor_tickable ... ok
test hs5_g_the_school_module_names_no_colour_the_palette_has_not_declared ... ok

test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

```
cargo test --features server --test pwa_tests

test the_phone_has_the_six_bottom_tabs_the_plan_names ... ok
...
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
```

### Transcript — HS6 (TV School panel)

```
cargo test --features server --test tv_tests

     Running tests\tv_tests.rs (target\debug\deps\tv_tests-555b723658b799fa.exe)

running 39 tests
test hs6_every_maximized_view_resolves_to_one_panel_slug ... ok
test hs6_f_left_and_right_wrap_over_the_four_panels ... ok
test hs6_a_the_golden_file_gains_one_school_section_and_moves_no_other ... ok
test hs6_c_a_boy_can_tick_every_lesson_with_the_remote_alone ... ok
test hs6_g_the_school_panel_never_renders_a_shared_subjects_row ... ok
test hs6_e_the_school_panel_did_not_move_the_type_scale_or_the_overscan ... ok
test hs6_h_a_parent_added_task_is_pinned_and_tickable_from_the_remote ... ok
test hs6_d_a_phone_can_steer_the_television_onto_the_school_panel ... ok
test hs6_b_every_school_row_is_within_twelve_presses_of_the_profile_selector ... ok
test t2_1_a_the_rendered_focus_order_matches_the_golden_file ... ok
[... 29 more pre-existing t2_1_*/d4_3_*/qd_02_*/qd_08_* rows, all ok, see the T2.1/D4.3 sections above ...]

test result: ok. 39 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
```

### Transcript — homeschool_loop_tests (HS7 accept a, b)

```
cargo test --features server --test homeschool_loop_tests

     Running tests\homeschool_loop_tests.rs (target\debug\deps\homeschool_loop_tests-ea9503191ff5ba07.exe)

running 1 test
test hs7_a_b_the_school_control_path_survives_two_surfaces_and_a_restart ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.13s
```

That one test walks the whole loop in order: a TV-side `toggle_lesson` (no
cookie) reaches a phone-tagged WS client inside 1 s; a phone-authed
`set_school_week` (Finish week) reaches a tv-tagged client inside 1 s; a
Together tick on a shared reading reaches both; the hub is killed and
restarted on the same address, both tagged clients reconnect and get `Hello`
inside the 30 s budget, and every write made before the kill — the week move,
the TV's tick, both boys' Together rows — is still there afterward, read back
from the same pool the (simulated) restart left running.

### Transcript — docs_tests (HS7 accept c)

```
cargo test --features server --test docs_tests

     Running tests\docs_tests.rs (target\debug\deps\docs_tests-6c0df583e4b77c63.exe)

running 17 tests
test fire_tv_doc_documents_all_three_branches ... ok
test fire_tv_doc_exists_with_status_line ... ok
test owner_checklist_has_a_device_row ... ok
test t3_2_fire_tv_covers_every_required_string ... ok
test t3_2_recovery_covers_at_least_four_named_failure_modes ... ok
test t3_2_owner_checklist_has_eight_numbered_steps_each_with_a_pass_criterion ... ok
test t3_2_the_runbooks_cross_reference_each_other ... ok
test test_dev_windows_md_exists_with_path_prefix ... ok
test test_non_rust_md_exists_with_required_content ... ok
test t3_2_every_runbook_doc_exists_and_is_substantial ... ok
test test_screensaver_assets_exist ... ok
test test_photo_fixture_has_sufficient_resolution ... ok
test test_tailwind_config_no_index_html ... ok
test t3_3_every_task_id_appears_exactly_once_in_verification ... ok
test test_pwa_icons_are_generated_with_correct_dimensions ... ok
test t3_2_every_internal_doc_link_resolves ... ok
test test_maskable_icons_have_ten_percent_safe_zone_padding ... ok

test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
```

`t3_3_every_task_id_appears_exactly_once_in_verification` passes with the
HS1–HS7 rows this section adds (HS8 excluded, exactly as `T3.5` is);
`t3_2_every_internal_doc_link_resolves` passes with `docs/homeschool/**` in
scope and no link into the gitignored `curriculum/` directory anywhere in
`docs/**`.

### Transcript — lib unittests (296 tests; HS-prefixed subset)

```
cargo test --features server --lib

     Running unittests src\lib.rs (target\debug\deps\family_calendar-bc87bcbce9ed8da6.exe)

running 296 tests
test client::components::glyphs::tests::every_homeschool_category_maps_to_a_non_ascii_glyph_and_unknown_falls_back ... ok
test client::components::homeschool::day_sheet::tests::the_not_dealt_out_line_is_exactly_what_the_plan_writes ... ok
test client::components::homeschool::day_sheet::tests::the_title_cap_matches_the_databases_own_check ... ok
test client::components::homeschool::day_sheet::tests::every_category_option_has_its_own_glyph ... ok
test client::components::homeschool::day_sheet::tests::an_extra_may_be_daily_reading_or_weekly_but_never_a_free_read ... ok
test client::components::homeschool::month::tests::an_empty_month_folds_to_no_weeks_at_all ... ok
test client::components::homeschool::month::tests::a_month_starting_mid_week_gets_leading_blanks_not_a_shifted_grid ... ok
test client::components::homeschool::month::tests::each_new_monday_opens_a_new_row ... ok
test client::components::homeschool::month::tests::every_day_of_the_month_lands_in_exactly_one_cell ... ok
test client::components::homeschool::month::tests::the_day_number_loses_its_leading_zero_but_never_its_digits ... ok
test client::components::homeschool::row::tests::a_split_reading_says_part_one_then_continue ... ok
test client::components::homeschool::row::tests::an_unsplit_reading_prints_no_part_label_at_all ... ok
test client::components::homeschool::row::tests::every_weekday_has_a_three_letter_name ... ok
test client::components::homeschool::row::tests::the_catch_up_chip_is_the_warm_hue_as_a_ground_under_dark_ink ... ok
test client::components::homeschool::settings::tests::a_profile_id_outside_the_rail_never_panics_the_sheet ... ok
test client::components::homeschool::settings::tests::the_sheet_is_a_modal_over_a_scrim_the_palette_already_declares ... ok
test client::components::homeschool::tests::a_date_yields_its_year_and_month_and_rejects_nonsense ... ok
test client::components::homeschool::tests::a_month_cursor_steps_across_a_year_boundary_in_both_directions ... ok
test client::components::homeschool::tests::a_month_label_names_the_month_in_words ... ok
test client::components::homeschool::tests::a_queued_tick_never_carries_a_profile_id_the_queue_cannot_hold ... ok
test client::components::homeschool::tests::the_month_view_always_falls_back_to_the_first_enrolled_boy ... ok
test client::components::homeschool::tests::today_is_the_tab_and_the_toggle_offers_only_the_other_two ... ok
test client::components::homeschool::tests::with_nobody_enrolled_there_is_no_boy_to_focus ... ok
test client::components::homeschool::today::tests::a_complete_week_nudges_towards_the_next_one ... ok
test client::components::homeschool::today::tests::a_fortnight_on_one_week_nudges_by_elapsed_days_instead ... ok
test client::components::homeschool::today::tests::a_paused_or_finished_year_never_nudges ... ok
test client::components::homeschool::today::tests::a_term_note_kind_never_renders_as_its_wire_string ... ok
test client::components::homeschool::today::tests::the_header_chip_reads_the_way_the_plan_writes_it ... ok
test client::components::homeschool::year::tests::a_cell_shows_the_first_eighteen_characters_and_says_so ... ok
test client::components::homeschool::year::tests::a_daily_row_with_no_assignment_rows_has_no_ordinals_to_recover ... ok
test client::components::homeschool::year::tests::every_grid_row_is_at_least_a_thumb_tall ... ok
test client::components::homeschool::year::tests::ordinals_come_from_first_appearance_across_the_week ... ok
test client::components::homeschool::year::tests::truncation_counts_characters_not_bytes ... ok
test server::homeschool::loader::tests::an_unknown_key_is_rejected_with_the_file_name_and_a_line_number ... ok
test server::homeschool::loader::tests::line_of_counts_newlines_before_the_offset ... ok
test server::homeschool::loader::tests::parse_days_rejects_unknown_letters_repeats_and_the_empty_string ... ok
test server::homeschool::loader::tests::parse_days_returns_the_letters_in_the_fixed_order ... ok
test server::homeschool::loader::tests::slug_is_valid_matches_lowercase_digits_and_hyphens_only ... ok
test server::homeschool::loader::tests::term_count_is_weeks_divided_by_term_weeks_rounded_up ... ok
test server::homeschool::loader::tests::shared_defaults_to_true_for_reading_and_weekly_and_false_for_daily ... ok
[... the 34 shared::homeschool::tests::hs3_* rows quoted above, 40 more
    homeschool-related rows (client::components::homeschool::*,
    server::homeschool::loader::*, the glyph test), and the remaining 222
    non-homeschool tests from every earlier phase — 296 total, all ok ...]

test result: ok. 296 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.52s
```

### `/health` — HS7 accept (e)

Two isolated boots of the real `target\debug\family-hub.exe run`, each on its
own port and its own fresh data directory (never the machine's live
`FamilyHub` Windows service):

```
# A: fresh, empty data directory — no curricula directory contents at all
$ Invoke-WebRequest http://127.0.0.1:8091/health
{"db":true,"last_google_poll":null,"cert_not_after":"2027-10-05T05:15:29+00:00",
 "days_to_expiry":396,"disk_free_bytes":196154327040,"ws_clients":0,
 "uptime_seconds":2,"migration_version":5,"curricula":0}

# B: FAMILY_HUB_CURRICULA_DIR pointed at the real, gitignored ao-year-1.toml
$ Invoke-WebRequest http://127.0.0.1:8093/health
{"db":true,"last_google_poll":null,"cert_not_after":"2027-10-05T05:18:07+00:00",
 "days_to_expiry":396,"disk_free_bytes":196087136256,"ws_clients":0,
 "uptime_seconds":2,"migration_version":5,"curricula":1}
```

`curricula: 0` with nothing in the directory, `curricula: 1` with the AO file
present — exactly HS7 accept (e). (This machine also runs a real, permanent
`FamilyHub` Windows service on the default ports for the owner's own
verification elsewhere in this project; both checks above ran on non-default
ports against throwaway data directories and never touched it.)

### `dx build --platform web --release` — HS7 accept (d)

```
dx build --platform web --release

  303.08s  INFO Compiled [303/303]: family-calendar
  303.16s  INFO Bundling app...
  303.60s  INFO Running wasm-bindgen...
  313.61s  INFO Copying asset (1/2): …\target\dx\family-calendar\release\web\public\wasm\family-calendar_bg.wasm
  314.08s  INFO Copying asset (2/2): …\target\dx\family-calendar\release\web\public\wasm\family-calendar.js
  314.42s  INFO Client build completed successfully! 🚀 path="…\target\dx\family-calendar\release\web\public"
  500.39s  INFO Compiled [648/648]: family-calendar
  500.48s  INFO Bundling app...
  500.65s  INFO Server build completed successfully! 🚀 path="…\target\dx\family-calendar\release\web"
```

Exit code 0 (500 s wall clock — `[profile.release]` in `Cargo.toml` pins
`lto = true, codegen-units = 1`, so the final link of both the wasm client
and the server binary is genuinely this slow, not a hang). Both halves
landed at `target\dx\family-calendar\release\web\` — `server.exe` and its
sibling `public\` folder, the exact layout `docs/OWNER_CHECKLIST.md` step
3/14 installs beside each other.

---

## Rendered in Chrome (School)

**Date:** 2026-09-03 (Boss pass on `main` at `2380542`, after HS1–HS7 landed)  
**Browser:** Chrome on the Windows 11 dev box, driven through the Claude-in-Chrome MCP tools (`tabs_context_mcp`, `tabs_create_mcp`, `navigate`, `resize_window`, `computer`, `read_console_messages`, `get_page_text`, `find`). **The browser tools were available**; everything below was observed, not inferred.  
**Build:** `cargo build --features server --release --bin family-hub` (4 m 13 s) plus `dx build --platform web --release` (264 s), then `Copy-Item -Recurse target\dx\family-calendar\release\web\public target\release\public -Force` — the exact `docs/DEV_WINDOWS.md` step 6 recipe (Q1-01). `target\release\family-hub.exe run` served `/tv` fully styled (hashed `tailwind-dxh….css` 200), so the Q1-01 fix holds for the plain-cargo binary once `public\` sits beside it.  
**Server:** started in the background with `FAMILY_HUB_DATA_DIR` pointed at a fresh temp directory whose only content was `curricula\ao-year-1.toml` (copied from the gitignored `docs/homeschool/curriculum/`; the `.expect.toml` sidecar and the source `.txt/.doc/.pdf` are not curriculum files, and the loader scans every `*.toml`, so only the real curriculum was copied). This machine also runs the owner's permanent `FamilyHub` Windows service on 8080/8443, so the test hub was bound to `FAMILY_HUB_ADDR=127.0.0.1:8095` / `FAMILY_HUB_TLS_ADDR=127.0.0.1:8096` instead of the URLs in the brief — same code path, different port; the live service was never touched (`Get-Service FamilyHub` → Running throughout, its listeners untouched).

### Boot seed and `/health`

`logs\familyhub.log` on the fresh data directory, in order: `scanning the curricula directory` → `loaded a curriculum file … curriculum_id=1 subjects=30 assignments=342 term_notes=29` → `finished loading curricula loaded=1 skipped=0` → **`enrollment seed: enrolled Isaiah at week 1 profile_id=1 curriculum_id=1 week_started_on="2026-09-03"`**.

```
$ curl http://127.0.0.1:8095/health           # before the TV tab
{"db":true,"last_google_poll":null,"cert_not_after":"2027-10-05T06:07:06+00:00",
 "days_to_expiry":396,"disk_free_bytes":224466718720,"ws_clients":0,
 "uptime_seconds":5,"migration_version":5,"curricula":1}
```

`curricula: 1`, `migration_version: 5`. `POST /api/get_enrollments` (HTTPS origin, `curl -k`) answered four rows — user 1 `enrolled: true, curriculum_name: "Ambleside Online Year 1", current_week: 1, weeks: 36, week_started_on: "2026-09-03", school_days: "MTWRF", paused: false`; users 2–4 `enrolled: false`. `POST /api/list_curricula` → one row, `slug: "ao-year-1", weeks: 36, term_weeks: 12, subject_count: 30`.

### `/tv` at 1920×1080 → School (one `Left` press)

Window resized to 1920×1080 via `resize_window` (captures report ~1522–1564 px wide because the display is DPI-scaled; the layout is the 1920-wide one). Fresh load lands on **Morning Routine · Isaiah**, styled, the four seeded boys in the left rail, `0 / 8`, no Disconnected badge. One `ArrowLeft` (`computer(action: "key")`) cycled the panel to **School** — Routine is panel 1 of 4 and School panel 4, so Left wraps straight to it as `docs/homeschool/PLAN_HOMESCHOOL.md` promises. Screenshot: [tv-school-week1-2026-09-03.jpg](verification/tv-school-week1-2026-09-03.jpg).

- Header: house glyph + **"School · Week 1"** in the T3.4 blue, "updated HH:MM" top-right.
- Left rail unchanged: Isaiah selected with the yellow focus ring, Nathaniel (truncated "Nathan…" at this width — the same truncation the routine panel shows), Simeon, scroll for Ezekiel.
- Right: a small-caps **TODAY** heading and the boy's own rows as large checkbox cards with a category glyph, title only (no AO text on the kiosk beyond the subject name), scrollable. `get_page_text` lists **6 rows**: Math, Handwriting / Copywork, Phonics / Reading practice, Recitation, Foreign language, Physical activity — i.e. the six *unshared* items of the ten `get_homeschool_today` reports as `due_today` for Isaiah on a Thursday; the other four are shared read-alouds, which W-16 keeps off the television (`his_own` in `src/client/components/tv/model.rs`) and shows under **Together** on the phone. No catch-up section (week 1, day 1: `catch_up: []`).
- Bottom: the four panel tabs (Morning Routine / Today / Whiteboard / School), **School** filled.
- Overscan margin visible on all four sides; nothing clipped; type readable at 10-ft scale.

### Console errors

`read_console_messages` across three full loads of `/tv` and the School panel: **zero errors, warnings or logs from the application.** The only entries are three `Error: A listener indicated an asynchronous response by returning true, but the message channel closed before a response was received` at `/tv:0:0` — the same installed-extension noise the 2026-08-30 pass attributed to the Acrobat extension, not the page. The hub's own log carried one WARN over the whole run: `client … idle for 90s; closing` (the realtime idle reaper doing its job while the tab sat between key presses; `ws_clients` was back to 1 afterwards).

### WebSocket

**Connected.** `/health` reported `"ws_clients":1` while the single `/tv` tab was open (0 before it was opened, and 1 again at `uptime_seconds: 228` after the idle close above), and the header never showed the red Disconnected badge.

### `/m` (phone surface) — School › Today / Year / Month

`http://127.0.0.1:8095/m` answered **308 → `https://127.0.0.1:8096/m`** (D3′; the redirect carries the configured TLS port, not a hard-coded 8443). The HTTPS URL hit Chrome's privacy interstitial for the hub's private CA (the CA is not installed in this profile, and installing it is the owner's step, not this pass's), and the MCP tools cannot attach to an interstitial (`Frame with ID 0 is showing error page` / `Cannot attach to this target`, so even the `thisisunsafe` keyboard bypass could not be typed). **The phone's School tab — Today, Year, Month — was therefore not screenshotted in Chrome in this pass**, exactly as in the 2026-08-30 pass. Verified out of band instead, against the same HTTPS origin the phone would use (`curl -k`):

| call | answer |
| --- | --- |
| `GET /m` | 200, `<title>Sheffield Family Hub</title>`, hashed Tailwind link |
| `POST /api/get_homeschool_today {"date":"2026-09-03"}` (Today) | `is_school_day: true, anyone_enrolled: true`, 1 group (week 1 of 36, term 1, `days_on_week: 0`, `can_finish_week: false`), **4 Together items**, Isaiah **10 due today / 0 catch-up / 0 done**, 11 term notes |
| `POST /api/get_week_grid {"user_id":1,"week":1}` (Year) | `week: 1, weeks: 36, term: 1, dated: true`, **5 day columns × 29 subject rows** |
| `POST /api/get_month {"user_id":1,"year":2026,"month":9}` (Month) | 30 day cells; `2026-09-03` is `in_current_week: true, week: 1, done: 0, total: 10`; the 1st and 2nd are school days with `week: null` (before the week started) |

So the three panes' data paths are live on the seeded hub; rendering them on a phone with the CA installed remains `docs/OWNER_CHECKLIST.md` step 14.

### Harness observation (not an app defect as far as this pass can tell)

After each fresh `navigate` to `/tv`, the first one or two `ArrowLeft` presses sent by the MCP tool were swallowed and only a later press switched the panel (pass 1: the press after a screenshot worked first time; pass 2: 2nd press; pass 3: 3rd press). The 2026-08-30 pass recorded the same shape as a focus artefact of the tool (keys go nowhere until the shell's `onmounted` autofocus has landed on the key-handler `<div tabindex="0">`, which needs hydration to finish first). Worth one glance on the real Fire TV during the owner checklist: if the first D-pad press after boot is ever ignored, that is this, not the School panel.

### Housekeeping

Background test hub stopped (`Stop-Process`; both test listeners gone, the real `FamilyHub` service still Running), the temp data directory left under the session scratchpad, the Chrome tab closed.
