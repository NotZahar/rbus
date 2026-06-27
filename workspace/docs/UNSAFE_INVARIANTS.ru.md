# Unsafe-инварианты

Unsafe допускается только в низкоуровневых местах: mmap, raw pointer доступ к
payload slots, `SCM_RIGHTS` ancillary data, futex syscall boundary и lifetime
binding `LoanedMessage`/`MessageRef`.

Каждый unsafe-блок должен иметь комментарий `SAFETY` на русском языке с инвариантами
и ссылками на тесты.

Проверенные зоны:

- `rbus-ipc::mmap`: pointer создаётся только из успешного `mmap`, освобождается
  через `munmap` в `Drop`; тест `mmap_region_allows_shared_read_write`.
- `rbus-ipc::scm_rights`: `msghdr`, `iovec` и `cmsghdr` живут на время
  `sendmsg`/`recvmsg`, `MSG_CTRUNC` проверяется; тест
  `scm_rights_sends_and_receives_file_descriptor`.
- `rbus-ipc::futex`: pointer берётся из `AtomicU32` и используется только на
  время syscall; тест `futex_wake_private_returns_zero_without_waiters`.
- `rbus-ipc::topic_wait`: `TopicWaitHeader` читается из mmap segment-а фиксированного
  размера, созданного `create_topic_wait_segment`; smoke benchmark
  `blocking_wait_wake_latency_ns` проходит через этот mapping.
- `rbus-ipc::stream_segment`: cleanup helpers получают `SegmentHeader`,
  `StreamCounters` и `SlotHeader` по offsets уже проверенного mmap segment-а;
  тест `rbusd_cleanup_clears_reader_bit_after_subscriber_crash`.
- `rbus::stream`: `SegmentHeader`, `StreamCounters`, `SlotHeader` и payload
  pointers берутся из offsets `rbus-core` внутри live mmap segment-а; writer
  синхронизируется через `state_sequence/readers_mask`; тесты
  `stream_initializes_shared_memory_segment_header`,
  `loaned_publish_makes_payload_visible_to_subscriber` и
  `message_ref_drop_releases_pinned_slot`.
