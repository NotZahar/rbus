# rbus

`rbus` — локальная шина сообщений для Linux-процессов в автомобильных и
робототехнических системах. Цель проекта: быстрая публикация/подписка через
разделяемую память, минимальный публичный Rust API и диагностика через
предупреждения

Текущий набор инструментов зафиксирован после проверки в контейнере Ubuntu 26.04:
`rustc 1.96.0 (ac68faa20 2026-05-25)`

## Ограничения

- Внешние зависимости промежуточного ПО во время выполнения не используются
- Сетевой транспорт не входит в исполняемую часть rbus
- Публичный API не содержит async, callback, serde, QoS builder, C/Python bindings

## Быстрый старт разработки

Все проверки выполняются через TypeScript-скрипт и Podman-контейнер:

```bash
npm run image
npm run check
```

Отдельные команды:

```bash
npm run fmt
npm run clippy
npm run test
npm run bench:smoke
npm run bench:reference
npm run bench:perf
npm run bench:strace
npm run hot-path:syscalls
```

`bench:reference` и `bench:perf` используют x86_64-эталон
`benches/baselines/rbus_suite_baseline_x86_64.csv`, собирают `rbusd` и
`rbus-bench` через `cargo build --profile bench` и запускают
`target/release/rbus-bench`

Матрица измерений использует 4 publisher-а, 8 subscriber-ов, пять режимов
транспорта и адаптивное число сообщений. Малый payload получает до 100000
сообщений на publisher, крупный payload ограничивается бюджетом 512 MiB payload
bytes на stream и лимитом `depth - 1`. Полный набор имеет предел времени
20 минут и завершается `Timeout`, если бюджет исчерпан перед очередным измерением

## Демон и CLI

`rbusd --domain default` запускает демон управляющей плоскости на переднем плане
и слушает Unix socket домена. Для короткой проверки без socket-а используется
`rbusd --domain default --smoke`

`Bus::connect("default")` подключается к уже запущенному `rbusd`; локальный
резервный реестр отсутствует. Publisher получает метаданные stream-а и memfd FD
от демона, subscriber получает `subscriber_id` и подключает streams через
`SCM_RIGHTS`. Payload после настройки идёт только через разделяемую память, не
через демон

`LoanedMessage` и `MessageRef` не клонируют `Arc<StreamInner>` на каждое
сообщение. `LoanedMessage` удерживает mutable borrow `Publisher`, а `MessageRef`
хранит pointer на уже подключенный stream, привязанный временем жизни. Второй
незавершённый loan запрещается типами, без `AtomicBool` в горячем пути

Для topic-а с одним подключенным stream-ом `Subscriber::try_recv` при наличии
сообщения идёт напрямую в ring read: без `stream_epoch` check, без
`RefCell<Vec<_>>` borrow и без scan stream set. Если чтение пустое, subscriber
проверяет `stream_epoch` и добирает новые publisher stream-ы

CLI-команды `list`, `stats`, `warnings`, `inspect`, `cleanup` и `doctor`
обращаются к `rbusd`. Если демон недоступен, команда завершается warning
`DaemonUnavailable` и кодом `2`

`list`/`inspect` читают активные метаданные реестра и `SegmentHeader` без payload
`stats` и `warnings` агрегируют `StreamCounters` из разделяемой памяти. `cleanup`
возвращает реально очищенные осиротевшие ресурсы. `doctor` проверяет
`/run/user/<uid>`, socket path, права, версии API/ABI, `memfd`, `mmap`,
`eventfd`, `futex`, `SCM_RIGHTS`, `MAX_SEGMENT_SIZE` и лимиты ресурсов

`rbus-cli bench-smoke` не обращается к реестру демона: команда запускает
`rbus-bench --smoke`, проверяет машиночитаемый отчёт и печатает его напрямую
С `--json` CLI добавляет внешний объект с `domain` и вкладывает отчёт в
`bench_smoke`

`rbus-cli --help` печатает общий список команд, а `rbus-cli <command> --help`
печатает справку по конкретной команде

## Очистка после падения и перезапуск

Очистка выполняется демоном по закрытию постоянной управляющей
сессии:

- биты погибшего subscriber-а очищаются из `readers_mask`
- stream погибшего publisher-а помечается закрытым в разделяемой памяти
- устаревший путь socket-а удаляется при перезапуске `rbusd`

Уже подключенная плоскость данных продолжает работать, если сам демон был убит и
запущен заново. Publisher хранит stream FD и topic-wait FD после `mmap`, поэтому
его управляющий worker повторно регистрирует старый stream в новом демоне через
`REREGISTER_PUBLISHER` без участия пути данных

Subscriber повторно регистрируется через `REREGISTER_SUBSCRIBER` на управляющем
или ожидающем пути и получает новый `subscriber_id`. Уже выданные `MessageRef` продолжают
освобождать старые reader bits при `Drop`

## API

```rust
use rbus::{Bus, TopicConfig};

# fn main() -> rbus::RbusResult<()> {
let bus = Bus::connect("default")?;
let mut publisher = bus.publisher("/camera/front/image", TopicConfig::new(4096, 64))?;
let subscriber = bus.subscriber("/camera/front/image")?;

let mut message = publisher.loan(4)?;
message.as_mut_slice().copy_from_slice(&[1, 2, 3, 4]);
message.publish()?;

if let Some(message) = subscriber.try_recv()? {
    let bytes = message.as_slice();
    assert_eq!(bytes, &[1, 2, 3, 4]);
}
# Ok(())
# }
```

## Документы

- [Архитектура](../docs/ARCHITECTURE.md)
- [API](../docs/API.md)
- [Версии API/ABI](../docs/VERSIONS.md)
- [Предупреждения](../docs/WARNINGS.md)
- [Тестирование](../docs/TESTING.md)
- [Benchmark-и](../docs/BENCHMARKS.md)
- [Эксплуатация](../docs/OPERATIONS.md)
- [Memory layout](../docs/MEMORY_LAYOUT.md)
- [Unsafe-инварианты](../docs/UNSAFE_INVARIANTS.md)
