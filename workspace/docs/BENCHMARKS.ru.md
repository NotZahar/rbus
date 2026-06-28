# Benchmark-и

`rbus-bench --smoke` выполняет короткую проверку benchmark harness и создаёт
machine-readable JSON-вывод. Полный multi-process suite собирается и запускается
из bench profile:

```text
cargo build --profile bench -p rbusd -p rbus-bench
target/release/rbus-bench --suite --format json
target/release/rbus-bench --suite --format csv
```

TypeScript-сценарии `bench:reference`, `bench:perf`, `bench:strace` и
`hot-path-syscall-smoke` тоже собирают `cargo build --profile bench -p rbusd -p
rbus-bench` и запускают `target/release/rbus-bench`, а не debug binary.
CSV suite пишет header перед первым измерением и flush-ит каждую завершенную
строку matrix сразу после расчета. Если прогон прерывается по `Timeout` или
regression gate падает после завершенных строк, уже записанная часть CSV
сохраняется. JSON suite остается full-report форматом и печатается только после
полного успешного завершения.

Smoke report содержит `status`, `warning_count`, количество сценариев,
transport-режимов и payload sizes, PID harness-а, флаг CPU pinning, topology
`publisher_count`/`subscriber_count`, `messages_per_second` и
`blocking_wait_wake_latency_ns`.

Актуальная suite matrix намеренно ограничена; сам suite имеет hard timeout 20
минут и завершает прогон ошибкой `Timeout`, если бюджет исчерпан перед очередным
измерением:

- topology: 4 publisher-а и 8 subscriber-ов, то есть 2 subscriber-а на каждого
  publisher-а;
- wait mode: `busy-poll`;
- payload: 64 B, 256 B, 1 KiB, 4 KiB, 16 KiB, 64 KiB, 256 KiB, 1 MiB, 4 MiB,
  8 MiB;
- transports: `rbus-loaned-metadata-only`, `rbus-loaned-fill`,
  `rbus-publish-bytes`, `unix-socket`, `unix-socket-end-to-end`.

Режимы разделены по смыслу:

- `rbus-loaned-metadata-only`: payload slots предварительно заполнены, в
  измерении обновляются только timestamp/metadata и выполняется commit;
- `rbus-loaned-fill`: пользователь пишет payload в loaned SHM buffer внутри
  измеряемого окна;
- `rbus-publish-bytes`: payload подготовлен до publish, измеряется одна копия в
  SHM;
- `unix-socket`: payload подготовлен до `write`;
- `unix-socket-end-to-end`: генерация payload входит в измеряемое окно.

Перед измерением publisher-и публикуют `depth` warmup-сообщений, а subscriber-ы
их вычитывают. Для rbus это pre-fault/pre-touch всех payload slots ring-а вне
измеряемого окна. Глубина ring-а покрывает весь measured message budget строки:
для small payload это до 100000 сообщений на publisher, а для large payload
количество сообщений ограничивается `depth - 1` из бюджета 512 MiB payload bytes
на stream. Такой budget не даёт валидному прогону перезаписать unread slots и
получить `MessagesSkipped`.

Readiness сделан двумя barrier-ами: subscriber пишет attach-ready file после
setup/attach, все worker-ы выполняют warmup и пишут measurement-ready file, затем
parent создаёт общий start flag. Worker-ы входят в measured window только после
start flag; fixed sleep для attach readiness не используется.

Suite report содержит две one-way latency-семантики:

- `transport_latency_*_ns`: timestamp ставится после подготовки payload;
- `end_to_end_latency_*_ns`: timestamp ставится перед генерацией payload.

Throughput считается по worker measured window: для one-way берётся максимум
`duration_ns` всех publisher/subscriber worker-ов после start barrier. Parent
wall-clock сохраняется отдельной метрикой `orchestration_wall_time_ns` и не
используется для throughput.

`syscalls/message` снимается отдельным `strace -c` pass той же строки matrix, но
для этого pass используется bounded sample: максимум 256 сообщений на publisher
и depth до 512, чтобы `strace` не доминировал над длительностью suite. Для
`rbus` + `busy-poll` suite фиксирует 0 `syscalls/message`; hot path
дополнительно проверяется marker-based strace smoke-тестом.

Быстрая проверка multi-process harness:

```text
target/release/rbus-bench --suite --matrix-smoke --messages 1 --format csv --no-gate
```

Baseline для regression gates:

```text
benches/baselines/rbus_suite_baseline_x86_64.csv
```

Baseline использует exact keys: `scenario`, `transport`, `wait_mode`,
`payload_size`, `publisher_count`, `subscriber_count`, `measured_messages`.
Wildcard-ключи и `payload_size=0` запрещены. Gate проверяет transport latency,
end-to-end latency, round-trip latency, throughput, payload throughput,
allocations/message, syscalls/message, warning count, skipped messages и
pinned-slot backpressure.

Контейнерные команды:

```bash
npm run bench:reference
npm run bench:perf
npm run bench:strace
```

`bench:reference` запускает suite с `--no-gate` и записывает CSV baseline.
Latency thresholds считаются как `max(2x, +50us)`, throughput thresholds как 80%
от измерения, `syscalls/message` ceiling как `1.25x + 16`. `bench:perf`
запускает suite с regression gate против baseline. Для короткой проверки можно
добавить `--matrix-smoke`.
