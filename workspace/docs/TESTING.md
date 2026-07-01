# Тестирование

Основная проверка:

```bash
npm run check
```

Она запускает fmt, clippy, tests, doc build, benchmark smoke,
multi-process benchmark suite smoke, CLI help checks и strace regression для
hot path

Короткая проверка benchmark suite:

```bash
npm run bench:suite-smoke
```

Она собирает `rbusd` и `rbus-bench` через bench profile, запускает matrix smoke,
поднимает 4 publisher-а и 8 subscriber-ов отдельными процессами и проверяет CSV
output harness-а

Benchmark-профили:

```bash
npm run bench:reference
npm run bench:perf
npm run bench:strace
```

`bench:reference` обновляет CSV baseline, `bench:perf` применяет regression
gates, `bench:strace` совмещает hot-path syscall smoke и короткий suite pass

## Policies

- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- workspace lint `warnings = "deny"`
- workspace lint `dead_code = "deny"`
- crate-level `allow(dead_code)` запрещён

## Покрытие

| Область | Где | Что проверяется |
| --- | --- | --- |
| Version contract | `crates/rbus-core/tests/version_contract_tests.rs` | API/ABI версии и ссылки на docs |
| Atomic protocols | `crates/rbus-core/tests/atomic_protocol_model_tests.rs` | reader pin, writer overwrite, ordering без `SeqCst` |
| Control codec | `crates/rbus-ipc/tests/control_codec_tests.rs` | binary frames и HELLO |
| IPC primitives | `crates/rbus-ipc/tests/*` | `memfd`, `mmap`, `SCM_RIGHTS`, futex, topic wait |
| Warning coverage | `crates/rbus-core/tests/warning_path_coverage_tests.rs` | каждый `WarningCode` имеет воспроизводимый path |
| Warning paths | `crates/rbus/tests`, `crates/rbus-ipc/tests` | counters/state меняются только ожидаемо |
| Cross-process data plane | `crates/rbus/tests/cross_process_data_plane_tests.rs` | real process publishers/subscribers |
| Examples | `examples/tests/examples_tests.rs` | examples работают через public API |
| Crash recovery | `crates/rbus/tests/crash_recovery_tests.rs` | dead subscriber, dead publisher, daemon restart |
| Allocations | `crates/rbus/tests/zero_copy_allocation_tests.rs` | 0 heap allocations in hot path |
| Refcount | `message_reservations_do_not_clone_stream_arc` | no per-message `Arc::clone` |
| False sharing | `stream_counters_do_not_share_cache_lines_between_write_heavy_fields` | counters split by cache line |
| Stream fast path | `crates/rbus/src/stream/tests.rs` | pin/claim, rollback, single-stream refresh |
| Syscalls | `workspace/scripts/hot-path-syscall-smoke.ts` | no syscalls between hot-path markers |
| Benchmark gates | `crates/rbus-bench/src/baseline.rs` | latency, throughput, allocations, syscalls |
| CLI | `crates/rbus-cli/tests` | help, success path, warning path, JSON validity |
| Daemon | `crates/rbusd/tests` | socket, register, attach, notify, unregister, cleanup |

## Важные сценарии

Subscriber до publisher-а проверяет `NOTIFY_STREAM_ADDED`. Subscriber после
publisher-а проверяет attach существующего stream-а. Fan-out проверяет доставку
одного сообщения нескольким subscribers

Crash recovery использует `fork` и `SIGKILL`. Subscriber crash удерживает
`MessageRef`, после чего `rbusd` должен очистить `readers_mask`. Publisher crash
проверяет `SEGMENT_FLAG_CLOSED` и `Stats.publisher_alive=false`

Daemon restart проверяет, что mmap data plane продолжает работать, а клиенты
повторно регистрируются в новом daemon-е

Compile-fail doctests фиксируют typestate: второй outstanding loan на том же
`Publisher` не компилируется, а `LoanedMessage` нельзя передать чужому publisher

`hot-path-syscall-smoke.ts` парсит только интервалы между
`RBUS_HOT_PATH_BEGIN:*` и `RBUS_HOT_PATH_END:*`; любой syscall внутри интервала
считается регрессией
