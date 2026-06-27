# Архитектура rbus

rbus разделён на public API, control plane и data plane.

Public API живёт в crate `rbus` и экспортирует только минимальный набор типов.
Control plane обслуживается `rbusd` и бинарным Unix socket protocol. Data plane
строится вокруг per-publisher stream и fixed-size SPMC ring.

Payload не передаётся через daemon. После setup/attach non-blocking publish и
try_recv не должны делать syscalls или heap allocations в steady state.

Blocking receive path остаётся внутренней data-plane деталью: daemon создаёт
topic-level wait segment, publisher будит его через futex после commit, а
subscriber ждёт этот futex в `recv`/`recv_timeout`. Public API не раскрывает и
не настраивает wait strategy. В topic wait segment отдельно хранится
`stream_epoch`: `rbusd` увеличивает его только при регистрации нового publisher
stream-а, поэтому subscriber отличает stream-added wake от обычного message wake
и не делает control-plane refresh в steady-state hot path.

Низкоуровневые Linux primitives находятся в `rbus-ipc`: `memfd_create`,
`ftruncate`, `mmap`, `eventfd`, `futex` и передача FD через `SCM_RIGHTS`.
`shm_open` fallback в текущей реализации отсутствует намеренно: неожиданный
отказ setup-path возвращается как `Warning`, а не переключается на другой
механизм.

`rbusd` создаёт `memfd` stream segment при `REGISTER_PUBLISHER`, инициализирует
`SegmentHeader`/`StreamCounters`/`SlotHeader` через общий helper
`rbus-ipc::stream_segment` и остаётся владельцем FD в registry. Publisher
получает daemon-issued `publisher_id`, `stream_id`, generation, metadata и FD
через `SCM_RIGHTS`, после чего только `mmap`-ит segment. Runtime publisher и
subscriber держат отдельную persistent control session открытой до `Drop` или
смерти process-а; `rbusd` привязывает registry entries к session id. Subscriber
получает daemon-issued `subscriber_id`, вызывает `ATTACH_STREAMS` и attach-ит
те же FD через `SCM_RIGHTS`. После первичного attach subscriber вызывает
`NOTIFY_STREAM_ADDED` с последней обработанной registry generation и получает
только новые stream metadata/FD.

`rbus::StreamInner` хранит attached stream metadata и payload в `memfd`-backed
`mmap MAP_SHARED` segment-е. Offsets, `SegmentHeader`, `StreamCounters` и
`SlotHeader` берутся из `rbus-core`, поэтому publish/recv path не имеет
отдельного heap-only layout и работает между разными process-ами.

В workspace включены `warnings = "deny"` и `dead_code = "deny"`. Неиспользуемые
placeholder-модули запрещены: модуль либо участвует в текущем срезе, либо не
компилируется в crate.

Subscriber хранит cursor на каждый attached stream. Если stream существовал до
регистрации subscriber-а, cursor ставится на текущий tail и исторический replay
не выполняется. Если stream появился после регистрации subscriber-а, cursor
начинается с первой sequence этого stream-а, чтобы subscriber не терял первое
сообщение нового publisher-а между control notification и следующим `try_recv`.
Если cursor отстал дальше глубины ring-а или slot уже содержит более новую
sequence, `try_recv` возвращает `WarningCode::MessagesSkipped`, но перед этим
сдвигает cursor stream-а на первую доступную committed sequence. Следующий
`try_recv` читает доступное сообщение, а не повторяет warning по уже потерянной
sequence.

`rbusd` запускает blocking accept-loop поверх Unix domain socket и создаёт
worker thread на каждое accepted соединение. HELLO возвращает версию control
protocol-а. One-shot CLI/diagnostic соединения закрываются после одного ответа.
Runtime REGISTER_PUBLISHER и REGISTER_SUBSCRIBER выполняются на persistent
session: закрытие socket-а считается lifecycle-событием и приводит к очистке
publisher stream-ов и subscriber id, принадлежащих этой session. ATTACH_STREAMS,
NOTIFY_STREAM_ADDED и UNREGISTER являются runtime control-plane командами для
публичного API `rbus`; INSPECT/GET_STATS/GET_WARNINGS/CLEANUP/DOCTOR формируют
диагностические ответы для `rbus-cli`. Эти ответы строятся из real registry
state и read-only snapshots active stream segment-ов: daemon читает
`SegmentHeader` и `StreamCounters`, но не payload area.
CLI не печатает локальные smoke-значения: при недоступном socket-е возвращается
`WarningCode::DaemonUnavailable`.

`UNREGISTER` удаляет publisher stream или subscriber id из daemon registry.
Автоматический cleanup закрытой session делает то же самое без участия клиента:
subscriber id очищает свой bit во всех `readers_mask` известных stream-ов,
publisher stream сначала помечается `SEGMENT_FLAG_CLOSED`, затем удаляется из
topic registry, daemon-owned FD закрывается. Уже attached data plane продолжает
читать mmap segment; subscriber видит закрытый stream через shared-memory flag
и не зависит от daemon для уже доставленных сообщений.

Если сам `rbusd` убит и запущен заново, старый daemon не успевает пометить
stream-ы closed. Уже созданные publishers/subscribers продолжают обмен через
существующие mmap mappings. Новый daemon при bind удаляет stale Unix socket
pathname только если по нему нельзя подключиться к живому listener-у.

Warning на runtime-команде считается fatal для текущего `rbusd` process-а:
daemon завершает request loop, печатает warning вместе с backtrace, а клиент
получает warning на чтении response.

Отдельный `rbusd --smoke` оставлен только как короткая проверка модулей
control-plane без bind socket-а.
