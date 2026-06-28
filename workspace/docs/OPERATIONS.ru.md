# Эксплуатация

`rbusd` запускается напрямую:

```bash
rbusd --domain default
```

Обычный запуск остаётся в foreground и bind-ит Unix `SOCK_SEQPACKET` socket domain-а:
`/run/user/<effective-uid>/rbus/<domain>.sock`. Каталог `/run/user/<uid>` должен
существовать, принадлежать этому UID и иметь права `0700`; socket directory
`/run/user/<uid>/rbus` создаётся daemon-ом с правами `0700`. Для быстрых проверок
без socket-а есть одноразовый режим:

```bash
rbusd --domain default --smoke
```

Диагностика выполняется через `rbus-cli doctor --domain default`; команды
`list`, `stats`, `warnings`, `inspect`, `cleanup` и `doctor` отправляют framed
control request в `rbusd`. Если daemon недоступен, CLI завершает работу с
`WarningCode::DaemonUnavailable` и exit code `2`; локального fallback-ответа с
нулевыми counters нет.

Каждая CLI-команда печатает собственную справку через
`rbus-cli <command> --help`; общий список команд остаётся доступен через
`rbus-cli --help`.

`rbus-cli bench-smoke` является обёрткой над `rbus-bench --smoke`, а не
заглушкой статуса. Команда запускает benchmark binary из того же каталога, что и
`rbus-cli`, требует успешный exit status и smoke JSON с `status`,
`warning_count`, matrix counters и `blocking_wait_wake_latency_ns`. Без `--json`
печатается исходный smoke report benchmark-а; с `--json` report вкладывается в
поле `bench_smoke` рядом с `domain`.

`rbus-cli list --domain <domain>` показывает active topics, publishers,
subscribers и streams из registry. Так как runtime protocol передаёт topic hash,
а не исходную строку имени, topic отображается как `topic_hash`.

`rbus-cli stats --domain <domain>` агрегирует реальные `StreamCounters` из
active shared-memory stream segment-ов: published/received/skipped messages,
backpressure, warning total, attach count и текущую write sequence.

`rbus-cli warnings --domain <domain>` показывает реальные warning counters и
bounded samples по stream-ам с ненулевым `warnings_total`. Samples не читают
payload и содержат только `topic_hash`, `stream_id` и counters.

При старте `rbusd` проверяет socket path domain-а. Если pathname остался после
убитого daemon-а и connect к нему невозможен, файл удаляется перед bind. Если
socket живой, cleanup не удаляет его и bind завершается системным warning-ом.

Inspect topic-а выполняется через `rbus-cli inspect --topic /name`. Если
`--topic` указан без значения, CLI завершает разбор аргументов с
`WarningCode::InvalidConfig` и exit code `2`. Inspect пересчитывает stable topic
hash на стороне daemon-а и выводит stream metadata: ids, generation, segment
size, slot size, depth, stream segment ABI, flags и write sequence. Payload не
читается.
Wait state хранится в отдельном topic-level wait segment-е и не является частью
stream inspect snapshot-а.

`rbus-cli cleanup --domain <domain>` очищает stale socket pathname, orphaned
registry records, пустые topics и reader bits для inactive subscriber ids.
Ответ показывает отдельные counters очистки.

`rbus-cli doctor --domain <domain>` проверяет `/run/user/<uid>`, права и owner
socket directory, socket path, public API version, control protocol version,
stream segment ABI version, topic wait segment ABI version, `memfd`
создание/seal, `mmap MAP_SHARED`, `eventfd`, private/shared `futex`,
`SCM_RIGHTS`, `MAX_SEGMENT_SIZE`, `RLIMIT_NOFILE`, `RLIMIT_MEMLOCK`,
`RLIMIT_FSIZE`, `RLIMIT_AS` и текущий warning counter. Любой failed check
возвращается как warning и завершает команду с exit code `2`.

Текущий `rbusd` принимает blocking Unix `SOCK_SEQPACKET` connections и
обрабатывает control frames на соединении. Runtime-команды `REGISTER_PUBLISHER`,
`REGISTER_SUBSCRIBER`, `ATTACH_STREAMS`, `NOTIFY_STREAM_ADDED` и `UNREGISTER`
используются библиотекой `rbus`; диагностические команды HELLO, INSPECT,
GET_STATS, GET_WARNINGS, CLEANUP и DOCTOR используются CLI. Payload через daemon
не проходит.

Для runtime-команд warning является fatal: daemon завершает обработку и процесс
выходит с кодом `2`. Клиент на этом соединении получает warning чтения response,
обычно `WarningCode::Disconnected`.

Runtime setup использует `memfd`/`mmap` и FD passing через `SCM_RIGHTS`.
Автоматического перехода на `shm_open` нет; такая деградация запрещена для
предсказуемой эксплуатации.

Crash cleanup выполняется по закрытию persistent control session:

- dead subscriber: `rbusd` очищает bit subscriber-а во всех известных
  `readers_mask`, включая slot, удержанный погибшим `MessageRef`;
- dead publisher: `rbusd` ставит `SEGMENT_FLAG_CLOSED`, увеличивает
  `disconnected_streams`, удаляет stream из registry и закрывает daemon-owned
  FD;
- restart `rbusd`: если daemon убит, он не маркирует существующие stream-ы, и
  уже attached data plane продолжает работать через сохранённые mmap mappings.

После restart-а daemon-а control-plane registry восстанавливается клиентами.
Publisher держит свои stream/topic-wait FD открытыми и внутренним worker-ом
повторно регистрирует старый stream в новом `rbusd`. Subscriber хранит
topic-wait FD и re-register-ится при следующем control refresh или blocking wait;
новый daemon выдаёт ему новый `subscriber_id`, после чего subscriber заново
attach-ит известные stream-ы без historical replay. Если daemon ещё недоступен,
уже attached publisher/subscriber продолжают data-plane обмен через старые mmap
segments; новые клиенты увидят старые stream-ы после успешного re-register
publisher-а.
