# API rbus

Публичный API ограничен типами:

- `Bus`
- `TopicConfig`
- `Publisher`
- `Subscriber`
- `LoanedMessage`
- `MessageRef`
- `Sequence`
- `Stats`
- `Warning`
- `WarningCode`
- `RbusResult`

Дополнительные public abstractions требуют отдельного обоснования и тестов.

`Bus::connect(domain)` подключается к `rbusd` через Unix domain socket
`/run/user/<effective-uid>/rbus/<domain>.sock`. Если daemon не запущен, метод возвращает
`WarningCode::DaemonUnavailable`; process-local registry и auto-start fallback
отсутствуют.

`Bus::publisher(topic, config)` регистрирует publisher в daemon-е. Daemon
выдаёт `publisher_id`, `stream_id`, stream generation и memfd FD. Publisher
attach-ит этот FD и дальше публикует payload напрямую в shared-memory ring.
Publisher держит persistent control session открытой до `Drop`; если process
умирает, `rbusd` видит EOF control socket-а, помечает stream closed в shared
memory и удаляет publisher stream из registry.

`Bus::subscriber(topic)` регистрирует subscriber в daemon-е и получает
`subscriber_id`. Streams topic-а attach-ятся через `ATTACH_STREAMS` и
`SCM_RIGHTS`; новые stream-и после первичного attach добираются через
`NOTIFY_STREAM_ADDED`. Subscriber держит persistent control session открытой до
`Drop`; после закрытия session `rbusd` освобождает `subscriber_id` для будущих
регистраций и очищает reader bit этого subscriber-а из уже опубликованных
slot-ов. Один stream поддерживает до 63 active subscribers; следующий
subscriber получает `WarningCode::SubscriberLimitReached`. Payload не проходит
через daemon.

`Subscriber` получает сообщения из stream-ов, attached к topic. Новый
subscriber не читает историю stream-ов, которые уже существовали до его
регистрации. Если publisher появляется после регистрации subscriber-а, первое
сообщение этого publisher-а должно быть доступно subscriber-у.

Crash publisher-а не инвалидирует уже attached `Subscriber`: stream остаётся
валидным mmap segment-ом, но `Stats.publisher_alive` для него становится
`false`. Crash subscriber-а с удержанным `MessageRef` очищается daemon-ом через
`readers_mask` sweep, поэтому publisher может снова использовать pinned slot.

`UNREGISTER` остаётся внутренней runtime-командой control protocol-а и не
расширяет публичный API crate `rbus`.

## Примеры

Исполняемые примеры вынесены в отдельный workspace package `examples` рядом с
`crates`. Cargo target-ы:

- `basic_pub` — публикация payload через `Publisher::publish_bytes`;
- `basic_sub` — blocking receive одного сообщения через `recv_timeout`;
- `loaned_pub` — zero-copy loan, запись в `LoanedMessage` и commit через
  `Publisher::publish`;
- `multi_sub` — последовательное чтение нескольких сообщений одного topic-а;
- `ping_pong` — round-trip из двух process-ов с ролями `server` и `client`;
- `slow_subscriber` — медленный consumer loop без fallback-ов и без скрытого
  запуска daemon-а.

Все примеры требуют уже запущенный `rbusd` и явные параметры `--domain`,
`--topic` или пары topic-ов. Любая ошибка API, timeout или warning завершает
пример с ненулевым кодом и stacktrace.
