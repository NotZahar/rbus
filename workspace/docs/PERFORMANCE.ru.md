# Производительность

Steady-state требования:

- loan + publish loaned: 0 heap allocations;
- try_recv при наличии сообщения: 0 heap allocations;
- try_recv без сообщения: 0 heap allocations;
- non-blocking publish/try_recv: 0 syscalls;
- loan/try_recv: 0 per-message `Arc` refcount операций;
- loan outstanding guard: compile-time mutable borrow, 0 per-loan `AtomicBool`;
- single-stream try_recv with message: 0 `stream_epoch` check, 0 `RefCell<Vec<_>>`
  borrow, 0 stream-set scan;
- publisher/subscriber counters: без false sharing в одной cache line;
- commit/rollback writer-claim clear: 0 RMW;
- payload copies in loaned path: 0.

Текущий `rbus` stream backend создаёт `memfd` и `mmap MAP_SHARED` на setup path.
После создания stream-а `loan`, `LoanedMessage::publish`, `try_recv` и `MessageRef::drop`
работают через atomics и pointer offsets внутри уже mapped segment-а.
`LoanedMessage` держит mutable borrow `Publisher`, а `MessageRef` держит
lifetime-bound pointer на attached stream, поэтому выдача и освобождение
сообщения не выполняют atomic increment/decrement `Arc` refcount-а.
Ограничение "один outstanding loan на publisher" выражено через `&mut Publisher`:
`Publisher::loan` и `Publisher::publish_bytes` принимают mutable receiver,
`LoanedMessage` удерживает этот borrow до `publish` или `Drop`, а runtime
`loan_in_progress: AtomicBool` отсутствует. Сам `Publisher` намеренно не `Sync`,
чтобы shared publisher не использовался несколькими потоками с дополнительной
синхронизацией в hot path.
`LoanedMessage::publish` также записывает monotonic timestamp в `SlotHeader` перед
commit. Этот timestamp path проверяется тем же syscall smoke: если получение
времени начнёт выполнять traced syscall между hot-path marker-ами, проверка
падает.

Slot protocol не использует `SeqCst`. Writer claim и reader pin синхронизируются
через acquire/release CAS на `readers_mask`; payload и metadata публикуются
release-store-ом в `state_seq`, а reader наблюдает их через acquire-load после
проверки `write_sequence`. Корректность interleaving-ов закреплена Loom-тестами
`reader_pin_and_writer_overwrite_are_mutually_exclusive` и
`commit_release_is_visible_after_observe_acquire`.
Reader pin имеет uncontended fast path: сначала выполняется один CAS
`0 -> reader_bit`, без предварительного load и CAS-loop. Если slot уже удерживает
другого reader-а, включается multi-reader fallback, который сохраняет fan-out
semantics. Writer claim снимается на commit/rollback через release-store `0`,
потому что reader не может легально добавить bit, пока writer claim удерживается.

`StreamCounters` использует cache-line sized cells: `published_messages`,
`received_messages`, `skipped_messages`, `backpressure_pinned_slot`,
`warnings_total`, `attach_count` и `disconnected_streams` лежат на разных
cache line. Publisher increment и subscriber increment не инвалидируют одну и
ту же line на каждом сообщении.
`published_messages` имеет одного writer-а и обновляется через process-local
monotonic cache плюс atomic store в shared counter, без shared `fetch_add`.
`received_messages` остаётся shared `fetch_add`, потому что один stream может
читаться несколькими subscribers, а daemon/CLI stats должны оставаться точными.

0 heap allocations в zero-copy hot path доказываются integration test-ом
`zero_copy_hot_path_does_not_allocate`. Тестовый бинарь устанавливает custom
global allocator, считает `alloc`, `alloc_zeroed` и `realloc`, прогревает setup
path вне измерения и затем проверяет три операции:

- `Publisher::loan` + `LoanedMessage::publish`;
- `Subscriber::try_recv` при наличии сообщения;
- `Subscriber::try_recv` без сообщения на уже attached stream-е.

Отсутствие per-message `Arc::clone` закреплено unit test-ом
`message_reservations_do_not_clone_stream_arc`: тест удерживает loan/read
reservation и проверяет, что `Arc::strong_count` stream-а не меняется.
Single-stream fast path хранит pointer на единственный `StreamInner` и cursor в
`Cell`; при наличии сообщения `Subscriber::try_recv` сразу выполняет ring read.
Если чтение пустое, только тогда проверяется shared `stream_epoch`, чтобы
подхватить новые publisher stream-ы. Regression
`single_stream_fast_path_refreshes_new_stream_after_empty_read` фиксирует, что
появление второго publisher-а после пустого fast-path чтения не теряется.
Отсутствие false sharing между hot counters закреплено layout test-ом
`stream_counters_do_not_share_cache_lines_between_write_heavy_fields`.
Pin/claim fast path закреплён unit tests
`uncontended_reader_pin_uses_empty_mask_path`,
`contended_reader_pin_keeps_multi_subscriber_fanout` и
`rollback_clears_writer_claim_without_reader_bits`.
Typestate loan guard закреплён compile-fail doctest-ом
`cannot_start_second_loan_before_first_is_finished`.

Blocking `recv`/`recv_timeout` не используют polling `sleep`. Subscriber ждёт
topic-level futex word в отдельном shared segment-е. Publisher после commit
читает `wake_waiters_mask`; при нулевой маске publish не пишет `wake_epoch` и не
делает `FUTEX_WAKE`. Если blocking subscriber уже выставил waiter bit, publisher
инкрементирует topic wake epoch и делает `FUTEX_WAKE`. Перед уходом в futex wait
subscriber повторно проверяет `stream_epoch` и write sequence attached stream-ов,
поэтому publish между последним `try_recv` и постановкой waiter-а не теряется.

0 syscalls в non-blocking hot path доказываются переносимым TypeScript
strace regression-скриптом `workspace/scripts/hot-path-syscall-smoke.ts`.
Он вычисляет `workspace/src` от собственного расположения и запускается через:

```bash
npm run hot-path:syscalls
```

Probe `rbus-bench --hot-path-syscall-probe publish` прогревает daemon setup,
registration, mmap attach и non-blocking subscriber attach до marker-а
`RBUS_HOT_PATH_BEGIN:publish`. Между BEGIN/END выполняется только steady-state
`Publisher::loan` + `LoanedMessage::publish`; `strace` не должен видеть ни одного
syscall-а.

Probe `rbus-bench --hot-path-syscall-probe try-recv` заранее публикует сообщения,
attach-ит stream и прогревает cursor. Между marker-ами выполняется только
steady-state `Subscriber::try_recv` на уже attached stream-е; любые syscall-и
между marker-ами считаются регрессией.

Benchmark suite дополнительно измеряет `syscalls/message` для полной
multi-process matrix. Это отдельный `strace -c` pass после основного timing pass,
поэтому latency и throughput regression gate не искажаются overhead-ом трассировки,
а syscall budget проверяется измерением реальных worker-процессов.
