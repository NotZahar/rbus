# Benchmark-и

Короткая проверка из `workspace/src`:

```text
cargo build --profile bench -p rbusd -p rbus-bench
target/release/rbus-bench --smoke
```

Полный suite:

```text
target/release/rbus-bench --suite --format json
target/release/rbus-bench --suite --format csv
```

npm-команды `bench:reference`, `bench:perf`, `bench:strace` и workspace-скрипт
`hot-path-syscall-smoke.ts` тоже используют bench profile и запускают
`target/release/rbus-bench`

## Режимы нагрузки

- `--load-mode saturated` — publisher-ы отправляют сообщения настолько быстро,
  насколько позволяет transport; основная метрика — максимальный throughput
- `--load-mode paced` — publisher-ы работают по абсолютному расписанию, сейчас
  поддерживается 30 Hz на publisher

## Матрица

Saturated suite:

- topology: 4 publisher-а и 8 subscriber-ов
- wait mode: `busy-poll`
- payload: 64 B, 256 B, 1 KiB, 4 KiB, 16 KiB, 64 KiB, 256 KiB, 1 MiB, 4 MiB, 8 MiB
- transports: `rbus-loaned-metadata-only`, `rbus-loaned-fill`,
  `rbus-publish-bytes`, `unix-socket`, `unix-socket-end-to-end`

Paced suite:

- topology: `one-publisher-one-subscriber`, `one-publisher-four-subscribers`,
  `four-publishers-eight-subscribers`
- wait mode: `busy-poll`
- payload: 64 B, 4 KiB, 64 KiB, 1 MiB, 8 MiB
- rate: 30 Hz на publisher
- transports: `rbus-loaned-metadata-only`, `rbus-loaned-fill`,
  `rbus-publish-bytes`, `unix-socket`

Suite имеет жесткий предел времени 20 минут; при исчерпании бюджета прогон
завершается `Timeout`

## Transport-режимы

- `rbus-loaned-metadata-only` — payload slots заранее заполнены, в измерении
  меняются только timestamp/metadata и commit
- `rbus-loaned-fill` — пользователь пишет payload в loaned SHM buffer внутри
  измеряемого окна
- `rbus-publish-bytes` — payload подготовлен до publish, измеряется одна копия в
  SHM
- `unix-socket` — payload подготовлен до `write`
- `unix-socket-end-to-end` — генерация payload входит в измеряемое окно

## Подготовка измерения

Перед измерением publisher-и публикуют `depth` сообщений прогрева, а subscriber-ы
их вычитывают. Для rbus это pre-fault/pre-touch всех payload slots ring-а вне
измеряемого окна

Глубина ring-а покрывает measured message budget строки. Это не даёт валидному
прогону перезаписать непрочитанные slots и получить `MessagesSkipped`

Готовность сделана через attach-ready и measurement-ready barrier-ы. Worker-ы
входят в measured window по общему start flag с monotonic timestamp

## Метрики

`transport_latency_*_ns` ставит timestamp после подготовки payload
`end_to_end_latency_*_ns` ставит timestamp перед генерацией payload

`actual_payload_write_gib_per_second` считает уникальную запись payload
publisher-ами. `effective_payload_delivery_gib_per_second` считает fan-out
доставку subscriber-ам и не является физической memory bandwidth

Для `rbus-loaned-metadata-only` GiB/s — это logical/effective метрика:
payload slot подготовлен заранее, а в измеряемом окне обновляются только
timestamp/metadata и commit

## Regression gates

Эталон:

```text
benches/baselines/rbus_suite_baseline_x86_64.csv
```

`bench:reference` пишет новый CSV-эталон без gate. `bench:perf` сравнивает
текущий прогон с эталоном. `bench:strace` добавляет проверку syscall budget

Gate проверяет latency, throughput, allocations/message, syscalls/message,
warning count, skipped messages и pinned-slot backpressure
