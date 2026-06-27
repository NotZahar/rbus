# Memory layout

Один stream segment состоит из:

1. `SegmentHeader`
2. `StreamCounters`
3. `SlotHeader[depth]`
4. `PayloadSlot[depth]`

`state_seq` кодирует sequence и writing bit: committed state хранит `sequence << 1`,
writing state хранит `(sequence << 1) | 1`.

Writer публикует payload и metadata до `Release` store в `state_seq`. Reader читает
`state_seq` через `Acquire` load, выставляет bit в `readers_mask`, затем повторно
проверяет `state_seq`.

`SegmentHeader.flags` является `AtomicU64`. Начиная с ABI v2 bit
`SEGMENT_FLAG_CLOSED` помечает stream закрытым после смерти publisher-а или
`UNREGISTER`. Уже attached subscribers читают этот flag из shared memory без
control-plane syscall; `Stats.publisher_alive` становится `false`, а
`StreamCounters.disconnected_streams` увеличивается daemon-ом при закрытии.

В ABI v3 `SegmentHeader` также содержит `wake_epoch` и `wake_waiters` для
совместимости layout-а с blocking wait evolution. Production blocking receive
использует отдельный topic-level wait segment размером в одну cache line:
`TopicWaitHeader { wake_epoch: AtomicU32, stream_epoch: AtomicU32,
wake_waiters_mask: AtomicU64, reserved }`. `wake_epoch` меняется при publish и
stream-added wake, а `stream_epoch` только при регистрации нового publisher
stream-а. Этот segment создаётся daemon-ом на topic, передаётся
publisher/subscriber через `SCM_RIGHTS` и mmap-ится `MAP_SHARED`.

Cleanup умершего subscriber-а выполняется вне hot path: `rbusd` mmap-ит
daemon-owned FD stream-а и очищает bit этого `subscriber_id` во всех
`SlotHeader.readers_mask`. Это освобождает pinned slots даже если `MessageRef`
не был dropped из-за crash process-а.

Layout инициализируется в `rbus-ipc::stream_segment` на стороне `rbusd` при
создании daemon-owned memfd. Клиентский `rbus::StreamInner` attach-ит полученный
FD через `mmap MAP_SHARED` и проверяет `SegmentHeader.magic`, `abi_version`,
`stream_id`, `publisher_id`, `topic_hash`, `slot_size`, `depth` и offsets на
boundary attach-а. Это покрыто тестом `stream_initializes_shared_memory_segment_header`.
