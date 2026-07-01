# Производительность

Требования устойчивого состояния:

- loan + publish loaned: 0 heap allocations
- `try_recv` с сообщением: 0 heap allocations
- `try_recv` без сообщения: 0 heap allocations
- неблокирующие publish/try_recv: 0 системных вызовов
- loan/try_recv: 0 операций `Arc` refcount на сообщение
- один незавершённый loan: compile-time mutable borrow, 0 `AtomicBool`
- single-stream `try_recv` с сообщением: без `stream_epoch` check, borrow
  `RefCell<Vec<_>>` и scan stream set
- publisher/subscriber counters: без false sharing в одной cache line
- commit/rollback writer-claim clear: 0 RMW
- payload copies in loaned path: 0

## Hot path

Stream backend создаёт `memfd` и `mmap MAP_SHARED` на setup path. После этого
`loan`, `LoanedMessage::publish`, `try_recv` и `MessageRef::drop` работают через
atomics и offsets внутри уже отображенного segment-а

`LoanedMessage` держит `&mut Publisher`, а `MessageRef` держит pointer на
attached stream, привязанный lifetime-ом к `Subscriber`. Поэтому per-message
`Arc::clone` не нужен

`Publisher` намеренно не `Sync`. Для параллельной публикации используются
отдельные publisher instances

`LoanedMessage::publish` записывает monotonic timestamp в `SlotHeader` перед
commit. Syscall smoke падает, если этот путь начнёт делать системный вызов в
hot-path interval

## Atomics

Slot protocol не использует `SeqCst`. Writer claim и reader pin используют
acquire/release CAS на `readers_mask`; payload и metadata публикуются
release-store-ом в `state_seq`

Reader pin сначала пробует CAS `0 -> reader_bit`. Если slot уже удерживает
другой reader, включается multi-reader fallback без потери fan-out semantics

Writer claim снимается на commit/rollback через release-store `0`, потому что
reader не может легально добавить bit, пока claim удерживается

## Counters

`StreamCounters` разнесены по cache-line sized cells:

- `published_messages`
- `received_messages`
- `skipped_messages`
- `backpressure_pinned_slot`
- `warnings_total`
- `attach_count`
- `disconnected_streams`

`published_messages` имеет одного writer-а и обновляется через process-local
monotonic cache плюс atomic store. `received_messages` остаётся общим
`fetch_add`, потому что один stream может читаться несколькими subscribers

## Проверки

0 heap allocations в zero-copy hot path доказывает
`zero_copy_hot_path_does_not_allocate`

Отсутствие per-message `Arc::clone` доказывает
`message_reservations_do_not_clone_stream_arc`

Single-stream fast path проверяется тестом
`single_stream_fast_path_refreshes_new_stream_after_empty_read`

False sharing между counters проверяется тестом
`stream_counters_do_not_share_cache_lines_between_write_heavy_fields`

Reader pin/claim fast path проверяется тестами
`uncontended_reader_pin_uses_empty_mask_path`,
`contended_reader_pin_keeps_multi_subscriber_fanout` и
`rollback_clears_writer_claim_without_reader_bits`

## Blocking receive

`recv`/`recv_timeout` не используют polling `sleep`. Subscriber ждёт futex word в
topic wait segment. Publisher делает `FUTEX_WAKE` только если есть waiters

Перед futex wait subscriber повторно проверяет `stream_epoch` и write sequence,
поэтому publish между последним `try_recv` и постановкой waiter-а не теряется

## Benchmark semantics

`actual_payload_write_gib_per_second` считает уникальную запись payload
publisher-ами

`effective_payload_delivery_gib_per_second` считает fan-out доставку
subscriber-ам. Для metadata-only это logical/effective metric, а не физическая
пропускная способность памяти
