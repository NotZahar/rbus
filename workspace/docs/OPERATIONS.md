# Эксплуатация

Запуск daemon-а:

```bash
rbusd --domain default
```

Обычный запуск остаётся на переднем плане и привязывает Unix `SOCK_SEQPACKET`
socket:

```text
/run/user/<effective-uid>/rbus/<domain>.sock
```

Каталог `/run/user/<uid>` должен принадлежать этому UID и иметь права `0700`
Каталог `/run/user/<uid>/rbus` создаётся daemon-ом с правами `0700`

Короткая проверка без socket-а:

```bash
rbusd --domain default --smoke
```

## CLI

Диагностика:

```bash
rbus-cli doctor --domain default
```

Команды `list`, `stats`, `warnings`, `inspect`, `cleanup` и `doctor` отправляют
framed управляющий запрос в `rbusd`. Если daemon недоступен, CLI завершает
работу с `WarningCode::DaemonUnavailable` и кодом выхода `2`

Справка доступна отдельно для каждой команды:

```bash
rbus-cli <command> --help
```

`rbus-cli bench-smoke` запускает `rbus-bench --smoke`, проверяет JSON короткой
проверки и печатает его напрямую или вкладывает в `bench_smoke` при `--json`

## Команды

- `list` показывает active topics, publishers, subscribers и streams из registry
- `stats` агрегирует реальные `StreamCounters` из active stream segment-ов
- `warnings` показывает warning counters и ограниченные samples
- `inspect --topic /name` выводит ids, generation, segment size, slot size,
  depth, ABI, flags и write sequence
- `cleanup` очищает stale socket pathname, orphaned registry records, пустые
  topics и reader bits неактивных subscribers
- `doctor` проверяет `/run/user/<uid>`, права, socket path, версии API/ABI,
  `memfd`, `mmap`, `eventfd`, `futex`, `SCM_RIGHTS` и resource limits

Payload через daemon не проходит

## Socket cleanup

При старте `rbusd` проверяет socket path domain-а. Если pathname остался после
убитого daemon-а и подключиться к нему невозможно, файл удаляется перед bind

Если socket живой, cleanup не удаляет его, а bind завершается системным warning

## Runtime-команды

Библиотека `rbus` использует:

- `REGISTER_PUBLISHER`
- `REGISTER_SUBSCRIBER`
- `ATTACH_STREAMS`
- `NOTIFY_STREAM_ADDED`
- `UNREGISTER`
- `REREGISTER_PUBLISHER`
- `REREGISTER_SUBSCRIBER`

Для runtime-команд warning является fatal: daemon завершает обработку и процесс
выходит с кодом `2`. Клиент обычно получает `WarningCode::Disconnected`

Настройка использует `memfd`/`mmap` и передачу FD через `SCM_RIGHTS`
Автоматического перехода на `shm_open` нет

## Crash cleanup и restart

Закрытие постоянной управляющей сессии запускает cleanup:

- dead subscriber: `rbusd` очищает bit subscriber-а во всех известных
  `readers_mask`
- dead publisher: `rbusd` ставит `SEGMENT_FLAG_CLOSED`, увеличивает
  `disconnected_streams`, удаляет stream из registry и закрывает daemon-owned FD
- restart `rbusd`: если daemon был убит, уже attached data plane продолжает
  работать через сохранённые mmap mappings

После restart-а клиенты повторно регистрируют старые FD. Publisher делает это
через `REREGISTER_PUBLISHER`, subscriber — через `REREGISTER_SUBSCRIBER`
