# Memory layout

Один stream segment состоит из:

1. `SegmentHeader`
2. `StreamCounters`
3. `SlotHeader[depth]`
4. `PayloadSlot[depth]`

Актуальные номера контрактов перечислены в
[Версии API/ABI](VERSIONS.ru.md): stream segment ABI сейчас `5`, topic wait
segment ABI сейчас `1`.

`state_seq` кодирует sequence и writing bit: committed state хранит `sequence << 1`,
writing state хранит `(sequence << 1) | 1`.

Writer публикует payload и metadata до release-store в `state_seq`. Reader
читает `write_sequence`/`state_seq` через acquire-load, CAS-ом выставляет свой
bit в `readers_mask`, затем повторно проверяет `state_seq`. Slot protocol не
использует `SeqCst`: writer claim и reader pin используют acquire/release CAS,
а commit виден через release-store в `state_seq` и `write_sequence`. Старший
bit `readers_mask` зарезервирован под writer claim, поэтому stream поддерживает
до 63 subscriber reader bits.
`SlotHeader.timestamp_ns` заполняется monotonic timestamp-ом на commit path до
release-store в `state_seq`; после успешного pin reader читает тот же metadata
snapshot через `MessageRef::timestamp_ns()`.

Перед выдачей loan-а writer CAS-ом переводит `readers_mask` из `0` в writer
claim bit, затем переводит `state_seq` в writing state. Claim bit удерживается
до commit или rollback, поэтому reader не может закрепиться за slot-ом, пока
publisher получил mutable payload.
Один outstanding loan на `Publisher` не хранится в shared memory и не защищается
runtime `AtomicBool`: `LoanedMessage` удерживает `&mut Publisher`, поэтому второй
loan на том же publisher-е не проходит borrow checker.
На commit/rollback writer снимает claim release-store-ом `0`, а не RMW-операцией:
при удержанном writer claim reader не может успешно добавить reader bit. Reader
pin сначала пробует uncontended CAS `0 -> reader_bit`; если другой reader уже
держит slot, включается fallback CAS-loop, который добавляет bit без потери
fan-out для нескольких subscribers.

`SegmentHeader.flags` является `AtomicU64`. Начиная со stream segment ABI `2`
bit `SEGMENT_FLAG_CLOSED` помечает stream закрытым после смерти publisher-а или
`UNREGISTER`. Уже attached subscribers читают этот flag из shared memory без
control-plane syscall; `Stats.publisher_alive` становится `false`, а
`StreamCounters.disconnected_streams` увеличивается daemon-ом при закрытии.

В stream segment ABI `4` stream-level поля `wake_epoch` и `wake_waiters` удалены
из `SegmentHeader`: они не участвовали в data plane и дублировали отдельный
topic-level wait segment. Blocking receive использует shared segment размером в
одну cache line с topic wait ABI `1`:
`TopicWaitHeader { magic: u64, abi_version: u32, reserved_header: u32,
wake_epoch: AtomicU32, stream_epoch: AtomicU32, wake_waiters_mask: AtomicU64,
reserved }`. `wake_epoch` меняется при publish и stream-added wake только если
`wake_waiters_mask != 0`, а `stream_epoch` только при регистрации нового
publisher stream-а. Этот segment создаётся daemon-ом на topic, передаётся
publisher/subscriber через `SCM_RIGHTS` и mmap-ится `MAP_SHARED`. При attach
проверяются `magic` и `abi_version`; mismatch завершает operation без fallback-а.

В stream segment ABI `5` каждый write-heavy counter внутри `StreamCounters`
занимает отдельную cache line. `published_messages` пишет publisher,
`received_messages` пишет subscriber, а warning/backpressure/gap counters не
лежат рядом с ними. Это убирает false sharing между publisher и subscriber
ядрами при каждом сообщении.
`published_messages` обновляется одним writer-ом через локальный monotonic cache
и atomic store в shared counter. `received_messages` остаётся atomic RMW, потому
что его могут одновременно обновлять несколько subscribers одного stream-а.

Cleanup умершего subscriber-а выполняется вне hot path: `rbusd` mmap-ит
daemon-owned FD stream-а и очищает bit этого `subscriber_id` во всех
`SlotHeader.readers_mask`. Это освобождает pinned slots даже если `MessageRef`
не был dropped из-за crash process-а.

Layout инициализируется в `rbus-ipc::stream_segment` на стороне `rbusd` при
создании daemon-owned memfd. Клиентский `rbus::StreamInner` attach-ит полученный
FD через `mmap MAP_SHARED` и проверяет `SegmentHeader.magic`, `abi_version`,
`stream_id`, `publisher_id`, `topic_hash`, `slot_size`, `depth` и offsets на
boundary attach-а. После attach hot path использует cached typed pointers,
полученные через checked projection `MappedRegion`; повторных mmap layout
проверок в `loan`/`publish`/`try_recv` нет. Это покрыто тестами
`stream_initializes_shared_memory_segment_header` и
`mmap_region_checks_typed_projection_bounds_and_alignment`.
