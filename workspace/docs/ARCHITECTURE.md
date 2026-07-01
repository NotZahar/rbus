# Архитектура rbus

rbus состоит из трёх частей:

- публичный API в crate `rbus`
- управляющая плоскость в `rbusd`
- плоскость данных в разделяемой памяти

Версии публичного API, управляющего протокола, stream segment ABI и topic wait
segment ABI зафиксированы в [Версиях API/ABI](VERSIONS.md)

## Основная модель

Payload не проходит через демон. Демон нужен для регистрации участников,
передачи FD и диагностики; после настройки publisher и subscriber работают через
`memfd` + `mmap MAP_SHARED`

Один publisher получает один stream segment. Topic может содержать несколько
stream-ов:

```text
topic "/camera/front/image"
 ├── stream #1 от publisher #1
 ├── stream #2 от publisher #2
 └── stream #3 от publisher #3
```

Stream — это per-publisher SPMC ring. Publisher пишет только в свой segment, а
subscribers читают attached stream-ы topic-а

## Управляющая плоскость

`rbusd` слушает Unix `SOCK_SEQPACKET` socket domain-а и обрабатывает binary
frames. Основные команды протокола времени выполнения:

- `REGISTER_PUBLISHER`
- `REGISTER_SUBSCRIBER`
- `ATTACH_STREAMS`
- `NOTIFY_STREAM_ADDED`
- `UNREGISTER`
- `REREGISTER_PUBLISHER`
- `REREGISTER_SUBSCRIBER`

Диагностические команды `INSPECT`, `GET_STATS`, `GET_WARNINGS`, `CLEANUP` и
`DOCTOR` используются `rbus-cli`

Каждый runtime-клиент держит постоянную управляющую сессию. Закрытие socket-а
считается событием жизненного цикла: daemon очищает subscriber id, закрытые
publisher stream-ы и reader bits

## Плоскость данных

`rbusd` создаёт stream segment через `memfd`, инициализирует
`SegmentHeader`/`StreamCounters`/`SlotHeader` и передаёт FD клиентам через
`SCM_RIGHTS`

Publisher и subscriber отображают segment через `mmap MAP_SHARED`. После этого
`loan`, `publish`, `try_recv` и `MessageRef::drop` работают через atomics,
offsets и pointers внутри уже отображенного segment-а

Неблокирующий hot path после настройки не должен выполнять системные вызовы и
выделять память в куче

## Чтение stream-ов

Subscriber хранит cursor на каждый attached stream. Если stream существовал до
регистрации subscriber-а, cursor ставится на текущий tail и исторический replay
не выполняется

Если stream появился после регистрации subscriber-а, cursor начинается с первой
sequence этого stream-а, чтобы не потерять первое сообщение нового publisher-а

Для частого случая одного stream-а `Subscriber` использует быстрый путь: прямой
pointer на `StreamInner`, cursor в `Cell`, без обхода набора stream-ов на
успешном чтении

## Blocking receive

`recv` и `recv_timeout` используют отдельный topic wait segment с futex word
Publisher будит wait segment после commit, а subscriber ждёт futex в
блокирующем вызове

`stream_epoch` хранится отдельно и меняется только при регистрации нового
publisher stream-а. Это позволяет отличать обычное пробуждение по сообщению от
изменения состава stream-ов

## Crash и restart

Падение subscriber-а очищается daemon-ом: `rbusd` проходит по known stream-ам и
снимает bit этого `subscriber_id` из `readers_mask`

Падение publisher-а помечает stream как `SEGMENT_FLAG_CLOSED`, увеличивает
`disconnected_streams` и удаляет stream из registry

Если сам `rbusd` перезапущен, уже отображенные mmap segments остаются валидными
Клиенты повторно регистрируют свои FD через `REREGISTER_*`, а плоскость данных
продолжает работать без изменения публичного API
