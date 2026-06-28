# Тестирование

Основные команды выполняются через Podman-контейнер:

```bash
npm run check
```

Команда запускает fmt, clippy, tests, doc build, benchmark smoke,
multi-process benchmark suite smoke и проверку CLI help. CLI integration tests
покрывают каждую команду: command-specific `--help`, success path с живым
`rbusd`, fatal warning path и валидность JSON-вывода через `serde_json`. Также
в `check` входит strace regression `hot-path-syscall-smoke`, доказывающий 0
syscalls в non-blocking publish/try_recv steady state.

Отдельная короткая проверка полного multi-process benchmark suite:

```bash
npm run bench:suite-smoke
```

Она собирает `rbusd` и `rbus-bench` через `cargo build --profile bench`, запускает
`target/release/rbus-bench --suite --matrix-smoke --messages 1 --format csv
--no-gate`, поднимает `rbusd`, 4 publisher-а и 8 subscriber-ов как отдельные
процессы и проверяет CSV output harness-а. CSV suite stream-ит header и строки
matrix с flush после каждой строки, поэтому при timeout или gate failure
сохраняются уже завершенные измерения.

Benchmark regression-профили запускаются отдельными TypeScript-сценариями:

```bash
npm run bench:reference
npm run bench:perf
npm run bench:strace
```

`bench:reference` обновляет CSV baseline из no-gate прогона и по умолчанию
пишет `benches/baselines/rbus_suite_baseline_x86_64.csv`,
`bench:perf` применяет regression gates к этому baseline, а `bench:strace`
совмещает hot-path syscall smoke и короткий suite pass, где
`syscalls/message` снимается через `strace -c`. Reference CSV хранит thresholds:
latency `max(2x, +50us)`, throughput 80% от измерения и syscall ceiling
`1.25x + 16`. Suite по умолчанию делает один запуск каждой строки и имеет hard
timeout 20 минут: при исчерпании бюджета перед очередным измерением прогон
завершается `Timeout`. Syscall pass выполняется один раз на строку matrix и
использует bounded sample: максимум 256 сообщений на publisher и depth до 512.
Перед измерением worker-ы проходят attach-ready barrier, publisher-ы
pre-fault/pre-touch ring depth сообщениями, а measured message budget не
превышает `depth - 1`, чтобы валидный прогон не порождал `MessagesSkipped`.
Все worker-ы ждут общий start flag и только после этого входят в measured
window.

Benchmark TypeScript-скрипты не содержат абсолютный container source path:
корень `workspace/src` вычисляется от расположения файла
`workspace/scripts/workspace-paths.ts`. Это фиксируется тестом
`benchmark_scripts_do_not_hardcode_container_source_root`.

Lint policy:

- `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
- workspace lint `warnings = "deny"`;
- workspace lint `dead_code = "deny"`;
- crate-level `allow(dead_code)` запрещён.

Loom/model tests находятся в `crates/rbus-core/tests/atomic_protocol_model_tests.rs`.
Они перебирают interleaving-и atomic data plane protocol-ов: reader pin против
writer overwrite, commit `Release` против observe `Acquire`, cleanup/drop против
одного reader bit. Тест `slot_protocol_orderings_do_not_require_global_seq_cst`
дополнительно фиксирует, что slot protocol не возвращается к `SeqCst`. Эти
тесты запускаются обычным `cargo test --workspace --all-features` и не требуют
отдельного feature flag.

Regression suite `crates/rbus/tests/cross_process_data_plane_tests.rs`
поднимает `rbusd`, publishers и subscribers как разные OS process-ы; parent
process только оркестрирует stdin/stdout child-ов и не участвует в data plane.
Покрыты subscriber до publisher-а через `NOTIFY_STREAM_ADDED`, subscriber после
publisher-а через attach уже существующего stream-а, fan-out одного сообщения на
несколько subscribers и чтение нескольких per-publisher stream-ов одного topic-а.
Publisher child держит control session открытой до явной команды `exit`, поэтому
lifecycle cleanup не удаляет stream раньше проверки subscriber-ом. Process-local
registry в этих тестах не может скрыть ошибку.

Example regression suite `examples/tests/examples_tests.rs` подключена к
workspace через package `rbus-examples`. Она поднимает настоящий `rbusd`,
запускает bin-target-ы `basic_pub`, `basic_sub`, `loaned_pub`, `multi_sub`,
`ping_pong` и `slow_subscriber` как отдельные OS process-ы и проверяет, что они
обмениваются payload через публичный API без fallback-ов.

Lifecycle registry проверяется отдельными тестами:
`subscriber_id_is_reused_after_session_close` закрывает subscriber session без
`UNREGISTER` и ждёт повторной выдачи того же `subscriber_id`;
`publisher_stream_is_removed_after_session_close` закрывает publisher session и
ждёт удаления stream-а из `ATTACH_STREAMS`.

Warning coverage разделён на два уровня. `warning_path_coverage_tests.rs`
фиксирует, что каждый `WarningCode` имеет отдельный воспроизводимый path-тест с
описанной проверкой counters/state. Реальные warning-path тесты находятся рядом
с владельцами поведения: data-plane counters в `crates/rbus/tests`, закрытый
stream branch в `crates/rbus/src/stream.rs`, IPC warning paths в
`crates/rbus-ipc/tests`, benchmark timeout в `crates/rbus-bench/tests`.

Crash recovery покрыт `crates/rbus/tests/crash_recovery_tests.rs`. Тесты
используют `fork` и `SIGKILL`: subscriber crash удерживает `MessageRef` на
single-slot stream-е, после чего parent ждёт, что `rbusd` очистит
`readers_mask` и publisher снова сможет публиковать; publisher crash проверяет
`SEGMENT_FLAG_CLOSED`, `Stats.publisher_alive=false` и отсутствие падения
subscriber-а; restart test убивает `rbusd`, запускает тот же domain заново через
stale socket cleanup и проверяет, что уже attached mmap data plane продолжает
передавать сообщения.

Allocation regression покрыт `crates/rbus/tests/zero_copy_allocation_tests.rs`.
Тестовый бинарь включает custom global allocator и доказывает 0 heap allocations
для `loan` + `publish`, `try_recv` с сообщением и `try_recv` без сообщения.
Per-message refcount regression покрыт unit test-ом
`message_reservations_do_not_clone_stream_arc`: loan/read reservation больше не
увеличивают `Arc::strong_count` attached stream-а.
False-sharing regression для counters покрыт layout test-ом
`stream_counters_do_not_share_cache_lines_between_write_heavy_fields`: каждый
write-heavy field `StreamCounters` начинается с новой cache line.
Reader pin/claim regression покрыт unit tests в `crates/rbus/src/stream/tests.rs`:
`uncontended_reader_pin_uses_empty_mask_path`,
`contended_reader_pin_keeps_multi_subscriber_fanout` и
`rollback_clears_writer_claim_without_reader_bits`.
`empty_try_recv_after_initial_attach_does_not_use_control_plane` дополнительно
убивает daemon после attach и проверяет, что пустой `try_recv` возвращает
`None`, а не пытается выполнить новый control request.
`single_stream_fast_path_refreshes_new_stream_after_empty_read` проверяет, что
single-stream fast path не пропускает второй publisher stream: после пустого
чтения первого stream-а новый stream attach-ится по `stream_epoch`, и сообщение
второго publisher-а доступно в том же `try_recv`.
Typestate regression для loan guard закреплён compile-fail doctest-ом в
`crates/rbus/src/loaned.rs`: второй outstanding loan на том же `Publisher` не
компилируется, потому что `LoanedMessage` удерживает `&mut Publisher` до
`publish` или `Drop`. Отдельный compile-fail doctest фиксирует, что
`Publisher::publish(LoanedMessage)` отсутствует и loan нельзя передать чужому
publisher-у.

Syscall regression покрыт `workspace/scripts/hot-path-syscall-smoke.ts`.
TypeScript-скрипт запускает `rbus-bench --hot-path-syscall-probe publish` и
`rbus-bench --hot-path-syscall-probe try-recv` под `strace`, парсит только
интервалы между marker syscall-ами `RBUS_HOT_PATH_BEGIN:*` и
`RBUS_HOT_PATH_END:*`, и падает при любом syscall-е внутри интервала.

Карта актуального покрытия:

| Фича | Файл | Тест |
| --- | --- | --- |
| Binary control frame codec | `crates/rbus-ipc/src/control/codec.rs` | `crates/rbus-ipc/tests/control_codec_tests.rs` |
| HELLO client header | `crates/rbus-ipc/src/control/client.rs` | `crates/rbus-ipc/tests/control_codec_tests.rs` |
| `memfd_create` sizing | `crates/rbus-ipc/src/memfd.rs` | `crates/rbus-ipc/tests/memfd_tests.rs` |
| `mmap MAP_SHARED` и checked typed projection | `crates/rbus-ipc/src/mmap.rs` | `crates/rbus-ipc/tests/mmap_tests.rs` |
| `SCM_RIGHTS` FD passing и seqpacket framing | `crates/rbus-ipc/src/{scm_rights.rs,unix_socket.rs,control/io.rs}` | `crates/rbus-ipc/tests/scm_rights_tests.rs` |
| `futex` wake/warning path | `crates/rbus-ipc/src/futex.rs` | `crates/rbus-ipc/tests/futex_tests.rs` |
| Topic wait mask не пишет wake epoch без waiters | `crates/rbus-ipc/src/topic_wait.rs` | `crates/rbus-ipc/tests/topic_wait_tests.rs` |
| WarningCode coverage catalog | `crates/rbus-core/src/warning.rs` | `every_warning_code_has_reproducible_warning_path_coverage` |
| IPC warning paths и state checks | `crates/rbus-ipc/src/{control,futex,mmap,permissions,stream_segment}.rs` | `crates/rbus-ipc/tests/warning_path_tests.rs` |
| Commit timestamp metadata | `crates/rbus/src/stream.rs` | `loaned_publish_makes_payload_visible_to_subscriber` |
| Version contract для public API, control protocol, stream ABI и topic wait ABI | `crates/rbus-core/src/constants.rs`, `docs/VERSIONS.ru.md` | `api_and_abi_versions_are_centrally_synced` |
| mmap-backed stream header и stream segment ABI v5 с cache-line counters | `crates/rbus/src/stream.rs`, `crates/rbus-core/src/{layout/segment.rs,stats.rs}` | `stream_initializes_shared_memory_segment_header`, `stream_segment_abi_has_cache_line_aligned_counters_without_header_wake_fields`, `stream_counters_do_not_share_cache_lines_between_write_heavy_fields` |
| Topic wait segment ABI v1 с magic/version header | `crates/rbus-ipc/src/topic_wait.rs` | `topic_wait_segment_header_exposes_current_abi`, `topic_wait_segment_rejects_wrong_abi_header` |
| Data-plane warning counters/state | `crates/rbus/src/{publisher.rs,subscriber.rs,stream.rs}` | `crates/rbus/tests/warning_path_tests.rs`, `disconnected_warning_path_increments_total_on_closed_stream` |
| Typestate loan guard без runtime `AtomicBool` | `crates/rbus/src/{loaned.rs,publisher.rs}` | compile-fail doctest `cannot_start_second_loan_before_first_is_finished`, `loaned_publish_makes_payload_visible_to_subscriber` |
| Atomic slot protocols | `crates/rbus-core/src/atomic/ordering.rs`, `crates/rbus/src/stream.rs` | `crates/rbus-core/tests/atomic_protocol_model_tests.rs` |
| Gap handling продвигает subscriber cursor | `crates/rbus/src/{stream.rs,subscriber.rs}` | `subscriber_cursor_moves_to_oldest_available_message_after_gap_warning` |
| Пустой `try_recv` не ходит в control plane | `crates/rbus/src/subscriber.rs` | `empty_try_recv_after_initial_attach_does_not_use_control_plane` |
| Single-stream `try_recv` без `RefCell<Vec<_>>`/scan на message path | `crates/rbus/src/{subscriber.rs,subscriber_streams.rs}` | `single_stream_fast_path_refreshes_new_stream_after_empty_read`, `zero_copy_hot_path_does_not_allocate` |
| 0 allocations в zero-copy hot path | `crates/rbus/src/{publisher.rs,subscriber.rs,stream.rs}` | `zero_copy_hot_path_does_not_allocate` |
| Нет per-message `Arc::clone` в loan/read reservation | `crates/rbus/src/{stream.rs,loaned.rs,message_ref.rs}` | `message_reservations_do_not_clone_stream_arc` |
| Нет false sharing между write-heavy counters | `crates/rbus-core/src/stats.rs` | `stream_counters_do_not_share_cache_lines_between_write_heavy_fields` |
| Reader pin fast path и writer-claim clear без RMW | `crates/rbus/src/stream.rs` | `uncontended_reader_pin_uses_empty_mask_path`, `contended_reader_pin_keeps_multi_subscriber_fanout`, `rollback_clears_writer_claim_without_reader_bits` |
| 0 syscalls в non-blocking hot path | `crates/rbus-bench/src/hot_path_syscall.rs` | `workspace/scripts/hot-path-syscall-smoke.ts` |
| Multi-process benchmark suite smoke | `crates/rbus-bench/src/{suite.rs,worker.rs}` | `bench_suite_matrix_smoke_outputs_csv` |
| Benchmark regression gates | `crates/rbus-bench/src/baseline.rs` | `full_baseline_entry_allows_matching_result`, `baseline_gate_rejects_latency_regression`, `baseline_gate_rejects_zero_payload_key` |
| Benchmark reference/perf/strace scripts | `workspace/scripts/benchmark-{reference,perf,strace}.ts`, `workspace/scripts/workspace-paths.ts` | `benchmark_reference_perf_strace_scripts_exist`, `benchmark_scripts_do_not_hardcode_container_source_root` |
| Benchmark recv_timeout warning path | `crates/rbus-bench/src/worker.rs` | `bench_worker_recv_timeout_warning_path_is_fatal` |
| Local IPC benchmark baselines | `crates/rbus-bench/src/transports/{baseline_ipc.rs,baseline_paths.rs,posix_queue.rs}` | `bench_suite_matrix_smoke_outputs_csv` |
| Cross-process rbus data plane | `crates/rbus/src/control.rs`, `crates/rbusd/src/registry.rs` | `crates/rbus/tests/cross_process_data_plane_tests.rs` |
| Workspace examples | `examples/*.rs`, `examples/src/lib.rs` | `examples/tests/examples_tests.rs` |
| Crash recovery data plane | `crates/rbus-ipc/src/stream_segment.rs`, `crates/rbusd/src/registry.rs`, `crates/rbus/src/stream.rs` | `crates/rbus/tests/crash_recovery_tests.rs` |
| rbusd smoke state | `crates/rbusd/src/daemon.rs` | `crates/rbusd/tests/daemon_start_tests.rs` |
| rbusd Unix socket HELLO/INSPECT | `crates/rbusd/src/daemon.rs` | `crates/rbusd/tests/control_plane_tests.rs` |
| REGISTER_PUBLISHER happy/warning | `crates/rbusd/src/daemon.rs`, `crates/rbusd/src/registry.rs` | `register_publisher_happy_path_returns_stream_fd`, `register_publisher_warning_path_rejects_bad_payload` |
| REGISTER_SUBSCRIBER happy/warning | `crates/rbusd/src/daemon.rs`, `crates/rbusd/src/registry.rs` | `register_subscriber_happy_path_returns_subscriber_id`, `register_subscriber_warning_path_rejects_bad_payload` |
| ATTACH_STREAMS happy/warning | `crates/rbusd/src/daemon.rs`, `crates/rbusd/src/registry.rs` | `attach_streams_happy_path_sends_registered_stream_fd`, `attach_streams_warning_path_rejects_bad_payload` |
| NOTIFY_STREAM_ADDED happy/warning | `crates/rbusd/src/daemon.rs`, `crates/rbusd/src/registry.rs` | `notify_stream_added_happy_path_sends_new_stream_fd`, `notify_stream_added_warning_path_rejects_unknown_subscriber` |
| UNREGISTER happy/warning | `crates/rbusd/src/daemon.rs`, `crates/rbusd/src/registry.rs` | `unregister_happy_path_removes_publisher_stream`, `unregister_warning_path_rejects_unknown_publisher` |
| rbusd lifecycle cleanup | `crates/rbusd/src/daemon.rs`, `crates/rbusd/src/registry.rs` | `subscriber_id_is_reused_after_session_close`, `publisher_stream_is_removed_after_session_close` |
| rbusd explicit cleanup orphan records | `crates/rbusd/src/cleanup.rs` | `cleanup_removes_orphaned_subscriber_mask_bit` |
| CLI без daemon-а не использует fallback | `crates/rbus-cli/src/control_client.rs` | `crates/rbus-cli/tests/list_tests.rs` |
| CLI command help для каждой команды | `crates/rbus-cli/src/{args.rs,output.rs}` | `every_cli_command_prints_command_help` |
| CLI success path с running daemon | `crates/rbus-cli/src/commands/*.rs`, `crates/rbusd/src/{inspect,stats,warnings,cleanup,doctor}.rs` | `every_cli_command_succeeds_with_running_daemon` |
| CLI warning path для каждой команды | `crates/rbus-cli/src/{args.rs,control_client.rs}` | `every_cli_command_warning_path_is_fatal` |
| CLI JSON validity для каждой команды | `crates/rbus-cli/src/commands/*.rs`, `crates/rbusd/src/{inspect,stats,warnings,cleanup,doctor}.rs` | `every_cli_command_json_output_is_valid` |
