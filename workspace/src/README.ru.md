# rbus

`rbus` — локальный message bus для Linux-процессов automotive/robotics. Цель проекта:
быстрый publish/subscribe через shared memory, минимальный публичный Rust API и
диагностика только через предупреждения.

Текущий toolchain зафиксирован после проверки в контейнере Ubuntu 26.04:
`rustc 1.96.0 (ac68faa20 2026-05-25)`.

## Ограничения

- Внешние middleware runtime-зависимости не используются.
- Сетевой транспорт не входит в runtime rbus.
- Public API не содержит async, callback, serde, QoS builder, C/Python bindings.

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

`bench:reference` и `bench:perf` по умолчанию используют x86_64 baseline
`benches/baselines/rbus_suite_baseline_x86_64.csv`, собирают `rbusd` и
`rbus-bench` через `cargo build --profile bench` и запускают
`target/release/rbus-bench`. Benchmark matrix использует топологию 4 publisher-а и
8 subscriber-ов, пять transport-режимов и adaptive message count: small payload
получает до 100000 сообщений на publisher, large payload ограничивается
бюджетом 512 MiB payload bytes на stream и лимитом `depth - 1`. Полный suite
имеет hard timeout 20 минут и завершает прогон ошибкой `Timeout`, если бюджет
исчерпан перед очередным измерением.

## Daemon и CLI

`rbusd --domain default` запускает foreground control-plane daemon и слушает
Unix socket domain-а. Для короткой проверки без socket-а используется
`rbusd --domain default --smoke`.

`Bus::connect("default")` подключается к уже запущенному `rbusd`; локальный
registry fallback отсутствует. Publisher получает stream metadata и memfd FD от
daemon-а, subscriber получает `subscriber_id` и attach-ит streams через
`SCM_RIGHTS`. Payload после setup идёт только через shared memory, не через
daemon.
`LoanedMessage` и `MessageRef` не клонируют `Arc<StreamInner>` на каждое
сообщение: `LoanedMessage` удерживает mutable borrow `Publisher`, а
`MessageRef` хранит lifetime-bound pointer на уже attached stream. Второй
outstanding loan запрещается типами, без `AtomicBool` на hot path.
Для topic-а с одним attached stream-ом `Subscriber::try_recv` при наличии
сообщения идёт напрямую в ring read: без `stream_epoch` check, без
`RefCell<Vec<_>>` borrow и без scan stream set. Если чтение пустое, subscriber
проверяет `stream_epoch` и добирает новые publisher stream-ы.

CLI-команды `list`, `stats`, `warnings`, `inspect`, `cleanup` и `doctor`
обращаются к `rbusd`. Если daemon недоступен, команда завершается warning-ом
`DaemonUnavailable` и кодом `2`. `list`/`inspect` читают active registry
metadata и `SegmentHeader` без payload, `stats` и `warnings` агрегируют
`StreamCounters` из shared memory, `cleanup` возвращает реально очищенные
orphaned resources, а `doctor` проверяет `/run/user/<uid>`, socket path, права
и owner socket directory, public API version, control protocol version, stream
segment ABI version, topic wait segment ABI version, `memfd`, `mmap`, `eventfd`,
`futex`, `SCM_RIGHTS`, `MAX_SEGMENT_SIZE` и resource limits.

`rbus-cli bench-smoke` не обращается к daemon registry: команда запускает
`rbus-bench --smoke`, проверяет machine-readable smoke report и печатает этот
report. С `--json` CLI добавляет внешний объект с `domain` и вкладывает report в
поле `bench_smoke`.

`rbus-cli --help` печатает общий список команд, а
`rbus-cli <command> --help` печатает справку по конкретной команде.

Crash cleanup выполняется daemon-ом по закрытию persistent control session:
dead subscriber bits очищаются из `readers_mask`, dead publisher stream
помечается closed в shared memory, а stale socket pathname удаляется при restart
`rbusd`. Уже attached data plane продолжает работать, если сам daemon был убит и
запущен заново. Publisher хранит stream FD и topic-wait FD после `mmap`, поэтому
его control worker re-register-ит старый stream в новом daemon-е через
`REREGISTER_PUBLISHER` без участия payload path. Subscriber re-register-ится
через `REREGISTER_SUBSCRIBER` на control/wait path и получает новый
`subscriber_id`; уже выданные `MessageRef` продолжают освобождать старые reader
bits при `Drop`.

## Минимальный API

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

- [Архитектура](../docs/ARCHITECTURE.ru.md)
- [API](../docs/API.ru.md)
- [Версии API/ABI](../docs/VERSIONS.ru.md)
- [Предупреждения](../docs/WARNINGS.ru.md)
- [Тестирование](../docs/TESTING.ru.md)
- [Benchmark-и](../docs/BENCHMARKS.ru.md)
- [Эксплуатация](../docs/OPERATIONS.ru.md)
- [Memory layout](../docs/MEMORY_LAYOUT.ru.md)
- [Unsafe-инварианты](../docs/UNSAFE_INVARIANTS.ru.md)
