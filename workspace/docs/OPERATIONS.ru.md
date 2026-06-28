# Эксплуатация

`rbusd` запускается как user-level daemon:

```bash
rbusd --domain default
systemctl --user start rbusd@default.service
```

## Установка systemd user unit

Unit-файлы лежат в `systemd/` внутри исходного дерева. Для установки в user
systemd без прав root:

```bash
install -Dm0644 systemd/rbusd@.service "$HOME/.config/systemd/user/rbusd@.service"
install -Dm0644 systemd/rbusd.socket "$HOME/.config/systemd/user/rbusd.socket"
systemctl --user daemon-reload
systemctl --user enable --now rbusd@default.service
systemctl --user status rbusd@default.service
test -S "${XDG_RUNTIME_DIR}/rbus/default.sock"
```

`rbusd@.service` является основным эксплуатационным unit-ом. Instance name
становится domain-ом: `rbusd@default.service` запускает
`rbusd --domain default`, `rbusd@robot.service` запускает domain `robot`.
Бинарь должен быть установлен в `%h/.cargo/bin/rbusd`, как указано в
`ExecStart`. После обновления бинаря достаточно выполнить:

```bash
systemctl --user restart rbusd@default.service
```

`rbusd.socket` устанавливается вместе с service unit-ом и фиксирует ожидаемый
default socket `%t/rbus/default.sock` с mode `0600`. В текущей реализации
daemon сам bind-ит Unix socket, поэтому обычный запуск выполняется через
`rbusd@<domain>.service`, а не через socket activation.

Отключение user unit-а:

```bash
systemctl --user disable --now rbusd@default.service
systemctl --user daemon-reload
```

Обычный запуск остаётся в foreground и bind-ит Unix socket domain-а:
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
size, slot size, depth, ABI, flags, wake state и write sequence. Payload не
читается.

`rbus-cli cleanup --domain <domain>` очищает stale socket pathname, orphaned
registry records, пустые topics и reader bits для inactive subscriber ids.
Ответ показывает отдельные counters очистки.

`rbus-cli doctor --domain <domain>` проверяет `/run/user/<uid>`, права и owner
socket directory, socket path, control protocol version, shared-memory ABI
version, `memfd` создание/seal, `mmap MAP_SHARED`, `eventfd`, private/shared
`futex`, `SCM_RIGHTS`, `MAX_SEGMENT_SIZE`, `RLIMIT_NOFILE`, `RLIMIT_MEMLOCK`,
`RLIMIT_FSIZE`, `RLIMIT_AS` и текущий warning counter. Любой failed check
возвращается как warning и завершает команду с exit code `2`.

Текущий `rbusd` принимает blocking Unix socket connections и обрабатывает один
control frame на соединение. Runtime-команды `REGISTER_PUBLISHER`,
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
