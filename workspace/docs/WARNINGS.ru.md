# Каталог предупреждений

Все штатные сбои rbus возвращаются как `Warning`. Публичного `RbusError` нет.

Коды предупреждений поддерживаются в `rbus-core::warning::WarningCode`; каждый код
имеет текстовое описание и тест на наличие описания. Отдельный coverage-catalog
`crates/rbus-core/tests/warning_path_coverage_tests.rs` фиксирует
воспроизводимый warning-path тест и проверку counters/state для каждого
`WarningCode`, чтобы новый код нельзя было добавить только с текстом.

`Warning` хранит код, компонент, операцию и compact context без heap allocation.
Русский текст вычисляется из `WarningCode`, а optional context-поля хранятся через
bitmask. Это сохраняет публичный тип `RbusResult<T> = Result<T, Warning>` без
boxing и без clippy warning `result_large_err`.

На syscall boundary `rbus-ipc` возвращает `Warning`:

- `SCM_RIGHTS` send/receive поверх `SOCK_SEQPACKET`: `SyscallFailed`,
  `ResourceLimit` или `ProtocolMismatch`;
- подключение CLI к Unix socket daemon-а: `DaemonUnavailable`;
- framed control IO: `Disconnected`, `SyscallFailed`, `ResourceLimit` или `ProtocolMismatch`;
- `futex` wait/wake: `SyscallFailed` или `ResourceLimit`;
- `memfd`, `mmap`, `eventfd`, `clock_gettime`: профильные syscall warning-и.

Fallback warning path для `shm_open` отсутствует, потому что сам fallback не
реализуется в текущем runtime.

`WarningCode::MessagesSkipped` означает, что subscriber cursor отстал от
per-publisher stream-а и часть sequence уже перезаписана ring buffer-ом. Перед
возвратом warning runtime сдвигает cursor этого stream-а на первую доступную
committed sequence, поэтому следующий `try_recv` может прочитать актуальное
сообщение вместо повторного warning по той же потерянной sequence.

Data-plane warning-path тесты находятся в
`crates/rbus/tests/warning_path_tests.rs` и `crates/rbus/src/stream.rs`.
Они проверяют, что `MessageTooLarge`, `BackpressurePinnedSlot`,
`MessagesSkipped` и `Disconnected` изменяют только ожидаемые
`StreamCounters`/state: `warnings_total`, `backpressure_pinned_slot`,
`skipped_messages`, `current_sequence`, `publisher_alive` и
`disconnected_streams`.

IPC warning-path тесты находятся в
`crates/rbus-ipc/tests/warning_path_tests.rs`. Они воспроизводят
`ProtocolMismatch`, `PermissionDenied`, `SyscallFailed`, `ResourceLimit`,
`MmapFailed` и `SegmentAbiMismatch` без изменения внешней среды и проверяют
сохранность затронутого состояния: frame bytes, futex word, pipe FD, blocking
file и исходный stream segment.

`WarningCode::Timeout` покрывается benchmark worker path-ом
`rbus-bench --suite-worker subscriber --wait-mode recv-timeout`: отсутствие
publisher-а приводит к fatal warning exit code `2` и stacktrace.
