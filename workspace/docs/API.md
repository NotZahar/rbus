# API rbus

Текущая версия публичного API берётся из версии пакета workspace и указана в
[Версии API/ABI](VERSIONS.md). Изменение публичных типов или их семантики
должно сопровождаться обновлением этого контракта

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

Дополнительные публичные абстракции требуют отдельного обоснования и тестов

`Bus::connect(domain)` подключается к `rbusd` через Unix domain socket типа
`SOCK_SEQPACKET` по пути `/run/user/<effective-uid>/rbus/<domain>.sock`. Если
демон не запущен, метод возвращает `WarningCode::DaemonUnavailable`;
локальный для процесса реестр и резервный автозапуск отсутствуют

`Bus::publisher(topic, config)` регистрирует publisher в демоне. Демон выдаёт
`publisher_id`, `stream_id`, поколение stream-а и memfd FD. Publisher подключает
этот FD и дальше публикует данные напрямую в ring в разделяемой памяти
Publisher держит постоянную управляющую сессию открытой до `Drop`; если процесс
завершается, `rbusd` видит EOF управляющего сокета, помечает stream закрытым в
разделяемой памяти и удаляет publisher stream из реестра
`Publisher::loan` и `Publisher::publish_bytes` принимают `&mut self`
`LoanedMessage` удерживает mutable borrow этого `Publisher`, поэтому второй
незавершённый loan на том же publisher-е невозможен на этапе компиляции
Commit выполняется методом `LoanedMessage::publish`; публичного
`Publisher::publish(LoanedMessage)` нет, поэтому loan нельзя передать чужому
publisher-у ещё до проверок во время выполнения
`Publisher` намеренно не `Sync`; параллельные producer threads должны иметь
отдельные экземпляры publisher

`Bus::subscriber(topic)` регистрирует subscriber в демоне и получает
`subscriber_id`. Streams topic-а подключаются через `ATTACH_STREAMS` и
`SCM_RIGHTS`; новые stream-и после первичного подключения добираются через
`NOTIFY_STREAM_ADDED`. Subscriber держит постоянную управляющую сессию открытой
до `Drop`; после закрытия сессии `rbusd` освобождает `subscriber_id` для будущих
регистраций и очищает reader bit этого subscriber-а из уже опубликованных
slot-ов. Один stream поддерживает до 63 активных подписчиков; следующий
subscriber получает `WarningCode::SubscriberLimitReached`. Данные не проходят
через демон

`Subscriber` получает сообщения из stream-ов, подключенных к topic. Новый
subscriber не читает историю stream-ов, которые уже существовали до его
регистрации. Если publisher появляется после регистрации subscriber-а, первое
сообщение этого publisher-а должно быть доступно subscriber-у
Пустой неблокирующий `try_recv` после первичного подключения не обращается к
демону: новые stream-ы добираются только когда общий `stream_epoch` изменился
Если подключен ровно один stream, `try_recv` сначала читает его напрямую через
быстрый путь для одного stream-а, без заимствования `Vec`, циклического обхода и
проверки `stream_epoch`; обновление управляющей плоскости выполняется только
после пустого чтения. `MessageRef::timestamp_ns()` возвращает монотонную метку
времени, записанную publisher при commit сообщения перед release-store в
`state_seq`

Падение publisher-а не инвалидирует уже подключенный `Subscriber`: stream
остаётся валидным mmap segment-ом, но `Stats.publisher_alive` для него
становится `false`. Падение subscriber-а с удержанным `MessageRef` очищается
демоном через проход по `readers_mask`, поэтому publisher может снова
использовать закрепленный slot

`UNREGISTER` остаётся внутренней командой времени выполнения в управляющем
протоколе и не расширяет публичный API crate `rbus`

## Примеры

Исполняемые примеры вынесены в отдельный пакет workspace `examples` рядом с
`crates`. Cargo target-ы:

- `basic_pub` — публикация данных через `Publisher::publish_bytes`;
- `basic_sub` — блокирующее получение одного сообщения через `recv_timeout`;
- `loaned_pub` — zero-copy loan, запись в `LoanedMessage` и commit через
  `LoanedMessage::publish`;
- `multi_sub` — последовательное чтение нескольких сообщений одного topic-а;
- `ping_pong` — round-trip из двух process-ов с ролями `server` и `client`;
- `slow_subscriber` — медленный consumer loop без резервных путей и без скрытого
  запуска демона

Все примеры требуют уже запущенный `rbusd` и явные параметры `--domain`,
`--topic` или пары topic-ов. Любая ошибка API, превышение времени или
предупреждение завершает пример с ненулевым кодом и stacktrace
