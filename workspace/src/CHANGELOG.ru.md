# Changelog rbus

## 0.1.0

- Создан workspace проекта.
- Зафиксирован Rust toolchain: `1.96.0`.
- Stream segment ABI version: `4`.
- Topic wait segment ABI version: `1`.
- Control protocol version: `2`.
- Добавлена warning-only модель и начальная структура crates.
- Включены workspace lints `warnings = deny` и `dead_code = deny`.
- Удалены dead-code placeholder-модули из `rbus`; `allow(dead_code)` больше не используется.
- Добавлена реальная передача FD через `SCM_RIGHTS` и тест с `memfd`.
- Добавлены futex wait/wake wrappers и тесты warning/wake path.
- `rbus::StreamInner` переведён с heap `Vec` slot storage на `memfd` + `mmap MAP_SHARED` + `rbus-core` ABI layout.
- `Bus::connect` теперь подключается к `rbusd` без process-local registry fallback; publisher/subscriber регистрируются в daemon-е, а stream FD передаются через `SCM_RIGHTS`.
- Добавлен regression test, где subscriber и publisher находятся в разных process-ах и обмениваются payload через daemon-registered memfd stream.
- Расширены cross-process integration tests: parent process только оркестрирует, а `rbusd`, publishers и subscribers запускаются отдельными process-ами; покрыты несколько subscribers, несколько publishers одного topic-а, subscriber до publisher-а и attach existing stream при subscriber после publisher-а.
- Topic wait segment получил отдельный `stream_epoch`, чтобы message wake не скрывал появление нового publisher stream-а у уже активного subscriber-а.
- Topic wait segment получил явные `magic` и ABI version; attach с чужим layout теперь завершается `SegmentAbiMismatch`.
- Shell-скрипт hot-path syscall regression заменён на TypeScript-скрипт без `awk`/`grep`/shell parsing.
- `rbus-bench --smoke` теперь использует scenario/transport/payload matrix metadata в JSON-отчёте.
- `rbus-cli bench-smoke` теперь запускает реальный `rbus-bench --smoke`, валидирует smoke JSON report и больше не печатает статическую заглушку `bench_smoke="ok"`.
- `shm_open` fallback удалён из runtime crate.
- `rbusd` переведён из smoke-only режима в foreground Unix socket daemon с HELLO/INSPECT/GET_STATS/GET_WARNINGS/CLEANUP/DOCTOR frame handling.
- Runtime control protocol daemon-а закрывает REGISTER_PUBLISHER, REGISTER_SUBSCRIBER, ATTACH_STREAMS, NOTIFY_STREAM_ADDED и UNREGISTER; для каждой команды добавлены happy-path и warning-path тесты.
- Subscriber refresh после первичного attach использует generation-filtered NOTIFY_STREAM_ADDED вместо повторного полного ATTACH_STREAMS.
- `rbus-cli` больше не печатает локальные fallback-нулевые ответы: диагностические команды обращаются к `rbusd` и возвращают `DaemonUnavailable`, если daemon недоступен.
- `rbus-cli list/stats/warnings/inspect/cleanup/doctor` теперь показывает реальные registry metadata, shared-memory counters, warning samples, cleanup counters и doctor checks вместо smoke-заглушек.
- `rbus-cli <command> --help` печатает справку по конкретной CLI-команде.
- CLI integration tests покрывают каждую команду: help, success path с живым `rbusd`, fatal warning path и JSON validity.
- Добавлен crash recovery: `rbusd` очищает dead subscriber bits в `readers_mask`, помечает dead publisher stream как closed и удаляет stale socket pathname при restart.
- Добавлены fork/kill tests для crash subscriber с удержанным `MessageRef`, crash publisher и restart `rbusd` без остановки существующего data plane.
- Добавлен re-register после restart `rbusd`: publisher повторно отдаёт старые stream/topic-wait FD новому daemon-у, subscriber получает новый `subscriber_id` вне hot path, а новый subscriber после restart-а может attach-иться к старому publisher stream.
- Добавлен custom allocator regression test, доказывающий 0 heap allocations для zero-copy `loan` + `publish`, `try_recv` с сообщением и `try_recv` без сообщения.
- Benchmark regression gates переведены на полный CSV-формат suite report; добавлены TypeScript-сценарии `bench:reference`, `bench:perf` и `bench:strace`.
- Benchmark baseline заменён на exact x86_64 matrix 64 B..8 MiB без `payload_size=0`; default regression gate читает `rbus_suite_baseline_x86_64.csv`, а legacy CSV оставлен как совместимое зеркало.
