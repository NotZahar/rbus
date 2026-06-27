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

Она запускает `rbus-bench --suite --matrix-smoke --messages 1 --format csv
--no-gate`, поднимает `rbusd`, publisher и subscriber как отдельные процессы и
проверяет CSV output harness-а.

Lint policy:

- `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
- workspace lint `warnings = "deny"`;
- workspace lint `dead_code = "deny"`;
- crate-level `allow(dead_code)` запрещён.

Loom/model tests находятся в `crates/rbus-core/tests/atomic_protocol_model_tests.rs`.
Они перебирают interleaving-и atomic data plane protocol-ов: reader pin против
writer overwrite, commit `Release` против observe `Acquire`, cleanup/drop против
одного reader bit. Эти тесты запускаются обычным `cargo test --workspace
--all-features` и не требуют отдельного feature flag.

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
| `mmap MAP_SHARED` | `crates/rbus-ipc/src/mmap.rs` | `crates/rbus-ipc/tests/mmap_tests.rs` |
| `SCM_RIGHTS` FD passing | `crates/rbus-ipc/src/scm_rights.rs` | `crates/rbus-ipc/tests/scm_rights_tests.rs` |
| `futex` wake/warning path | `crates/rbus-ipc/src/futex.rs` | `crates/rbus-ipc/tests/futex_tests.rs` |
| Commit timestamp metadata | `crates/rbus/src/stream.rs` | `loaned_publish_makes_payload_visible_to_subscriber` |
| mmap-backed stream header | `crates/rbus/src/stream.rs` | `stream_initializes_shared_memory_segment_header` |
| Atomic slot protocols | `crates/rbus-core/src/atomic/ordering.rs`, `crates/rbus/src/stream.rs` | `crates/rbus-core/tests/atomic_protocol_model_tests.rs` |
| Gap handling продвигает subscriber cursor | `crates/rbus/src/{stream.rs,subscriber.rs}` | `subscriber_cursor_moves_to_oldest_available_message_after_gap_warning` |
| 0 allocations в zero-copy hot path | `crates/rbus/src/{publisher.rs,subscriber.rs,stream.rs}` | `zero_copy_hot_path_does_not_allocate` |
| 0 syscalls в non-blocking hot path | `crates/rbus-bench/src/hot_path_syscall.rs` | `workspace/scripts/hot-path-syscall-smoke.ts` |
| Multi-process benchmark suite smoke | `crates/rbus-bench/src/{suite.rs,worker.rs}` | `bench_suite_matrix_smoke_outputs_csv` |
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
