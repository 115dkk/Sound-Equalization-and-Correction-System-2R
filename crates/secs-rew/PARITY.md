# REW adapter parity

P22 implementation handoff. Source is this repository's pinned SECS 2 Python tree (GPL-3.0-only); no external source code was imported. `tests/data/recorded.json` contains the `StubSession` protocol replies from `tests/test_rewapi_real.py:52–173`. Its IR is the three-sample fixture from that file's `test_get_ir_decodes_big_endian_float32`, encoded big-endian.

## Service-facing API

- `RewClient<T: Transport>::new(transport)` makes no network calls. Default real transport: `HttpTransport::default()` → `http://127.0.0.1:4735`, plain HTTP, no TLS, proxy or redirects. Connect timeout is 600 ms; ordinary read 5 s; sweep command 180 s. Upstream's per-endpoint overrides are retained (version 2 s, probe 1 s, measurement list 10 s, IR 15 s, meter 3 s).
- `Transport::request(&self, &Request) -> Result<Response, RewError>`; requests carry method, path, JSON body, ordered query pairs, timeout. `RecordedTransport` compares the entire next request and panics on mismatch, even if the client normally suppresses a connection error. Tests also call `assert_finished()`.
- `RewStatus`, `DeviceLister`, `LevelMonitor`, `Measurer`, `IRProvider` are implemented by both `RewClient` and `FakeRew`. They are Rust capability interfaces, not IPC definitions.
- Status: `is_running`, `version`, `probe`, `hold_offline`, `pro_state`, `is_pro`, `set_pro`. The Pro hint is never inferred from `/version`.
- Device/configuration: `driver`, `output_devices`, `input_devices`, `output_channel_names`, `input_channel_names`, `set_output_channel`, `set_timing_ref_output`, `set_blocking`, `set_sample_rate`, `set_sweep`, `set_level`, `set_repetitions`, `set_timing_reference`, `set_naming`, `set_notes`, `set_sequential_channels`, `configure`.
- Measurement: `run_position(&MeasurementPlan, pos, Option<ProgressCallback>, &CancelToken) -> Result<Vec<MeasurementResult>, RewError>`, `start_sweep`, `abort`, `wait_for_new_measurement`, `sweep_wait_s`, `measurements_map`, `measurements`, `measurement_ids`.
- Results: `impulse_response` returns `ImpulseResponse { ir: Vec<f32>, sample_rate, t0_s }`; `get_ir` returns `(Vec<f32>, u32)`. `MeasurementResult` includes role, position, optional UUID, name, IR, sample rate, t0, optional SPL/SNR/SPL offset. `spl`, `snr_db`, `spl_offset_db`, `measure_level_dbfs`, `input_calibration_file`, `process_result` retain upstream fallback behavior.
- Callbacks: `start`, `stop`, `last_levels`, `play_pink`, `stop_generator`, `subscribe_progress`, `unsubscribe_progress`, `close`. Handlers are `Arc<dyn Fn(...) + Send + Sync>`, invoked on callback threads; they must return promptly. `callback_url` prepares a random local URL so recorded scripts can expect the exact subscriber body. Pure parsers: `parse_progress`, `level_payload`, `b64_be_f32`, `channel_name_for`.
- `CancelToken` is clonable; cancellation returns already completed channels and emits terminal `cancelled`, matching upstream. It is checked between polls and in fake delays. A synchronous HTTP call already in progress remains bounded by its endpoint timeout, not instantaneously interrupted. An application needing immediate remote cancellation can issue `abort` from a separate client while cancelling the token.
- `FakeRew<S: IrSource>` accepts `IrSource::impulse_response(role, pos, sample_rate)` and optional `snr_db`. The source owns sample restrictions, synthesis, position variation and resampling. Defaults retain upstream fake status, devices, 50 ms sweep and 20 ms level period. Five `running` events span 18–90%; result data is cached by fake UUID. `fake_plan` is a minimal adapter plan helper.
- The adapter's `MeasurementPlan` is independent of the project crate (which this crate must not depend on). P23 maps P20's project plan into it. `plan_from_project` and multiamp project expansion belong to P20, not this crate.
- `launch::{find_rew_exe, launch, launch_external, launch_command, clean_env, ProcessProbe, rew_process_running}`. Launch uses `-api`, macOS uses `open -a … --args -api`; native Rust has no PyInstaller DLL search state. `ProcessProbe` is implemented by the application/P24, not this crate. Windows console suppression uses safe std API only.
- Errors: `Connection`, `Http { status, path, message }`, `Schema { path, message }`, `ProRequired { status, path }`, `Cancelled`; `.code()` returns stable `rew.*` identifiers. Upstream-style optional read methods intentionally suppress errors. Explicit close performs remote stop/unsubscribe; Drop only closes local callbacks and does not send generator Stop during a sweep.

## Endpoint provenance

All line references below are `src/secs/rewapi/rew.py` in the pinned tree.

| Method | Endpoint | Upstream line |
|---|---|---|
| GET | `/version` | 325, 334, 342 |
| GET | `/audio/driver` | 396 |
| GET | `/audio/asio/devices` | 129–130, 433 |
| GET | `/audio/java/output-devices`, `/audio/java/input-devices` | 137–138, 433 |
| GET | `/audio/asio/outputs`, `/audio/asio/inputs` | 131–132, 468 |
| GET | `/audio/java/outputs`, `/audio/java/inputs` | 139–140, 468 |
| POST | `/audio/asio/output`, `/audio/java/output` | 133, 141, 503 |
| GET | `/audio/java/ref-output-channels` | 143, 519 |
| POST | `/audio/asio/ref-output`, `/audio/java/ref-output-channel` | 134, 142, 524 |
| POST | `/application/blocking` | 535 |
| POST | `/audio/samplerate` | 539 |
| POST | `/measure/sweep/configuration` | 543–547 |
| GET, POST | `/measure/level` | 701, 550 |
| GET, POST | `/measure/sweep/repetitions` | 556, 560 |
| POST | `/measure/timing/reference` | 569 |
| POST | `/measure/naming` | 577–579 |
| POST | `/measure/notes` | 582 |
| POST | `/measure/sequential-channels` | 591–592 |
| GET | `/measurements` | 618 |
| POST | `/measure/command` (`SPL`, `Cancel`) | 645–646, 651 |
| GET | `/measurements/{uuid}/impulse-response?normalised=false` | 663–664 |
| GET | `/measurements/{uuid}` (SNR, SPL offset) | 679, 687 |
| GET | `/audio/input-cal` | 718 |
| GET | `/spl-meter/{meter}/levels` | 736–737 |
| POST | `/input-levels/subscribe` | 908–911 |
| POST | `/input-levels/command` (`Start`, `Stop`) | 912, 927 |
| POST | `/input-levels/unsubscribe` | 928 |
| GET | `/input-levels/last-levels` | 921 |
| POST | `/generator/signal` | 992 |
| POST | `/generator/signals/sine/configuration` | 994 |
| POST | `/generator/level` | 995 |
| POST | `/generator/command` (`Play`, `Stop`) | 996, 1000 |
| POST | `/measure/subscribe`, `/measure/unsubscribe` | 1013, 1036 |
| GET | `/measurements/process-result` | 1048 |

Configuration order (`rew.py:596–610`): blocking → sample rate → sweep → level → (timing then repetitions for repetitions >1; repetitions then timing otherwise) → optional timing-reference output. Repetition POST is skipped when GET reports the desired value.

Per-role order (`rew.py:798–836`): output → name → notes → baseline measurement IDs → SPL command → poll new measurement IDs → IR → SPL meter → SNR → SPL offset. Newest means highest measurement-list index, not lexicographically greatest UUID. Own progress subscriptions are removed even on errors. Progress relay holds a shared current-role lock; clearing it before process/done prevents late running events after completion.

## Test correspondence

Every name in the first column has the `test_` prefix in Rust. Unless otherwise specified, the Python test has the same name and prefix in the indicated upstream file. Additional assertions cover the fixed deviations and transport contract.

### `tests/rewapi_real.rs` → upstream `tests/test_rewapi_real.py`

| Rust name without `test_` | Upstream test / source |
|---|---|
| is_running_and_version | same |
| pro_state_only_comes_from_set_pro | same |
| configure_sends_documented_payloads | same |
| configure_repetitions_before_acoustic_and_timing_before_multiple | configuration regression, `rew.py:552–610` |
| sweep_length_is_mapped_to_a_rew_name | same |
| driver_is_queried_once_and_cached | same |
| output_channel_uses_rew_channel_names | same |
| output_channel_matches_asio_number_prefix | same |
| output_channel_falls_back_to_number | same |
| devices_are_plain_string_lists | same |
| sequential_channels_uses_channel_list_object | same |
| timing_ref_output_uses_driver_specific_body | `rew.py:505–525` |
| r1_pro_required_uses_only_status_and_measure_command_path | `test_pro_required_error_on_403`, R1 adversarial body/path/status matrix |
| http_error_raises_rew_error | same |
| get_ir_decodes_big_endian_float32 | same, plus malformed base64/length |
| impulse_response_reports_start_time_as_t0 | same |
| missing_start_time_falls_back_to_zero | same |
| measurements_map_is_keyed_by_position | same, numeric 2 vs 10 ordering |
| run_position_sequence_and_ir | same; asserts full request sequence and SNR/SPL offset |
| run_position_waits_until_the_measurement_appears | same |
| wait_for_new_measurement_stops_on_cancel | same |
| run_position_respects_cancel | same |
| abort_uses_the_cancel_command | same |
| parse_progress_reads_the_real_callback_strings | same |
| level_payload_reads_input_levels_arrays | same; SPL −180 and exact clipping boundary |
| play_pink_uses_sine_for_subs | same |
| last_levels_is_available_as_a_polling_fallback | same |
| measure_level_dbfs_reads_the_real_shape | same |
| input_calibration_file_empty_loaded_and_per_input | `test_input_calibration_file_empty_means_not_loaded`, `test_input_calibration_file_returns_the_path_when_loaded`, `test_input_calibration_file_is_none_when_per_input_files_differ` |
| spl_minus_180_is_none_and_missing_snr_is_none | `test_rew_progress_snr.py::test_missing_snr_is_none`, `rew.py:730–746` |
| recorded_transport_rejects_wrong_json | new strict-transport regression |
| recorded_transport_rejects_wrong_order | new strict-transport regression |
| rew_progress_strings_reach_the_progress_callback | `test_rew_progress_snr.py::test_rew_progress_strings_reach_the_progress_callback`, via actual HTTP callback delivery |
| r2_full_position_ignores_offline_hold_including_device_lookups | R2 full-session regression |
| cancel_during_running_aborts_and_keeps_terminal_phase_last | `rew.py:814–825`, cancellation while callbacks are arriving |

### `tests/rew_down_fast_fail.rs`

| Rust name without `test_` | Upstream test / source |
|---|---|
| second_request_fails_without_touching_the_network | `test_rew_down_fast_fail.py`, same; also `test_rewapi_real.py::test_is_running_false_when_rew_is_down` |
| retries_after_the_window_and_uses_a_short_connect_timeout | `test_rew_down_fast_fail.py`, same |
| probe_ignores_the_hold_and_reopens_it_when_rew_is_down | `test_rew_deferred_connect.py`, same (non-Qt portion) |
| r2_probe_circuit_breaker_does_not_block_measurement_requests | R2: SPL and Cancel still reach transport |
| measure_level_dbfs_and_input_calibration_file_are_none_when_down | `test_rewapi_real.py::test_measure_level_dbfs_is_none_when_rew_is_down`, `test_input_calibration_file_is_none_when_rew_is_down` |

### `tests/rewapi_callback.rs` → upstream `tests/test_rewapi_callback.py`

| Rust name without `test_` | Upstream test / source |
|---|---|
| server_starts_on_free_port_and_builds_urls | same, tokenized URLs per R3 |
| route_receives_json_body | same |
| query_string_and_unknown_path | same; unknown routes now 404 per R3, no catch-all handler |
| handler_exception_does_not_kill_server | same (Rust unwind caught) |
| history_and_get_health | same |
| stop_closes_port_and_drop_closes_port | `test_stop_closes_port`, `test_context_manager` |
| r3_random_token_wrong_path_404_and_oversized_body_drained | R3: rejects oversized body, then accepts next request on same connection |
| reply_is_sent_before_dispatch | `callback.py:57–59`, handler waits for client to receive reply |
| level_monitor_subscribes_with_callback_url_and_unsubscribes_on_close | `test_rewapi_real.py::test_level_monitor_subscribes_with_callback_url`, `test_progress_subscription_is_unsubscribed_on_close` |

### `tests/rewapi_fake.rs` → upstream `tests/test_rewapi_fake.py`

| Rust name without `test_` | Upstream test / source |
|---|---|
| fake_satisfies_all_protocols | same (compile-time bounds, also real client) |
| status_and_devices | same |
| plan_validate_and_naming | same |
| split_role_and_sweep_length | same |
| r4_fake_two_position_plan_uses_only_supplied_ir_source | `test_run_position_returns_ir_per_role`, `test_running_phase_is_sent_during_a_sweep`, `test_phases_include_running`; R4 source data/t0/SNR and two positions |
| ir_is_deterministic_and_position_dependent | same; variation belongs to supplied source per R4 |
| ir_provider_round_trip_and_unknown_id | `test_ir_provider_round_trip` |
| cancel_stops_after_current_role | same |
| delay_s_controls_sweep_time | same |
| level_monitor_and_generator | same |
| level_without_generator_is_noise_floor | same |
| measure_level_dbfs_and_calibration_when_running_or_down | `test_measure_level_dbfs_and_calibration_when_running`, `test_measure_level_dbfs_and_calibration_default_to_no_file`, `test_measure_level_dbfs_and_calibration_are_none_when_not_running` |

### Other tests

| Rust test | Upstream test / source |
|---|---|
| `proc_launch::test_clean_env_drops_the_bundle_from_path` | `test_proc_launch.py`, same |
| `proc_launch::test_clean_env_is_untouched_outside_a_bundle` | `test_proc_launch.py`, same |
| `proc_launch::test_launch_external_runs_in_the_programs_own_folder` | `test_proc_launch.py`, same; tests command specification, not an installed application |
| `proc_launch::test_find_rew_exe_searches_standard_install_locations` | `rew.py:148–156` |
| `proc_launch::test_rew_process_running_uses_injected_probe` | `rew.py:162–172`, process-list seam replaces subprocess tasklist |
| `http_transport::test_http_transport_sends_json_and_query_and_preserves_error_status` | real loopback wire regression for `rew.py:278–297` |
| `http_transport::test_http_transport_read_timeout_is_enforced` | real loopback timeout regression for `rew.py:276–280` |

## Deviations and boundary notes

R1–R4 are implemented and linked to tests in `docs/DEVIATIONS.md`. R1 intentionally confines 401/402/403 Pro detection to `/measure/command`, never text. R2 isolates probe failure state from a running measurement, including channel lookups. R3 uses a random token namespace, a 4 MiB body cap, bounded headers, drain-and-413 behavior, and registered-route-only dispatch. Callback framing is Content-Length as used by REW; unsupported chunked/ambiguous framing is rejected and the connection closed. The callback server uses only std plus the JSON parser already used by the client; no server or OS crate was added. R4 puts all data ownership in `IrSource`; no test module or embedded sample data is in the production crate.

Not performed: real REW/device sweeps, installed-application launch, Windows/macOS runtime validation, or Qt behavior. Project-plan extraction/re-export and multiamp expansion are P20 scope; Qt sidebar/watchers and low-SNR warning presentation are UI/service scope. P24 must supply process enumeration. The caller must provide sample IRs to FakeRew. `check_feature_tests.py` is intentionally reserved for the parent gate.
