# Unsafe-инварианты

Unsafe допускается только в низкоуровневых местах: mmap, доступ через raw
pointer к payload slots, ancillary data для `SCM_RIGHTS`, граница futex syscall
и привязка времени жизни `LoanedMessage`/`MessageRef`

Каждый unsafe-блок должен иметь комментарий `SAFETY` на английском языке с инвариантами
и ссылками на тесты

Проверенные зоны:

- `rbus-ipc::mmap`: pointer создаётся только из успешного `mmap`, освобождается
  через `munmap` в `Drop`; typed projection проверяет границы и alignment до
  передачи pointer-а владельцу layout-а; тесты
  `mmap_region_allows_shared_read_write` и
  `mmap_region_checks_typed_projection_bounds_and_alignment`
- `rbus-ipc::scm_rights`: `msghdr`, `iovec` и `cmsghdr` живут на время
  `sendmsg`/`recvmsg`, `MSG_CTRUNC` проверяется; тест
  `scm_rights_sends_and_receives_file_descriptor`
- `rbus-ipc::unix_socket`: raw `sockaddr_un` заполняется проверенным pathname
  без NUL-байтов, FD после успешного `socket` передаётся во владение
  `UnixListener`/`UnixStream`; тесты управляющей плоскости проходят через живой
  `rbusd`, а seqpacket framing проверяет `scm_rights_tests`
- `rbus-ipc::futex`: pointer берётся из `AtomicU32` и используется только на
  время системного вызова; тест `futex_wake_private_returns_zero_without_waiters`
- `rbus-ipc::topic_wait`: `TopicWaitHeader` pointer проверяется при map segment-а
  фиксированного размера, созданного `create_topic_wait_segment`; короткая
  benchmark-проверка `blocking_wait_wake_latency_ns` проходит через это mapping
- `rbus-ipc::stream_segment`: helpers очистки получают `SegmentHeader`,
  `StreamCounters` и `SlotHeader` через checked typed projection уже
  проверенного mmap segment-а;
  тест `rbusd_cleanup_clears_reader_bit_after_subscriber_crash`
- `rbus::stream`: `SegmentHeader`, `StreamCounters`, `SlotHeader` и payload
  pointers проверяются один раз при подключении и дальше хранятся как
  кэшированные pointers внутри живого mmap segment-а; writer синхронизируется через
  `state_sequence/readers_mask`; тесты
  `stream_initializes_shared_memory_segment_header`,
  `loaned_publish_makes_payload_visible_to_subscriber` и
  `message_ref_drop_releases_pinned_slot`
- `rbus::LoanedMessage`: mutable borrow `Publisher` удерживает подключенный
  stream живым и запрещает второй незавершённый loan на этапе компиляции; raw
  payload pointer используется только в пределах зарезервированного slot-а; регрессия
  закреплена doctest-ом `cannot_start_second_loan_before_first_is_finished`
- `rbus::MessageRef`: pointer на `StreamInner` берётся из подключенного stream-а,
  которым `Subscriber` владеет через `Arc`; stream-и не удаляются из subscriber
  set, пока жив сам `Subscriber`, поэтому `MessageRef<'subscriber>` может
  освобождать reader bit через pointer без per-message `Arc::clone`. Регрессия
  закреплена тестом `message_reservations_do_not_clone_stream_arc`
