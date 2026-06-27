# Benchmark-и

`rbus-bench --smoke` выполняет короткую проверку benchmark harness и создаёт
machine-readable JSON-вывод. Полный multi-process suite запускается отдельным
режимом:

```text
cargo run -p rbus-bench -- --suite --format json
cargo run -p rbus-bench -- --suite --format csv
```

Smoke report содержит:

- `status`;
- `warning_count`;
- количество сценариев benchmark matrix;
- количество transport baseline-ов;
- количество payload sizes;
- PID процесса harness;
- флаг CPU pinning;
- smoke topology `publisher_count`/`subscriber_count`;
- smoke `messages_per_second`;
- `blocking_wait_wake_latency_ns` для одного futex wait/wake на topic wait
  segment-е.

Контрольный прогон в контейнере разработки:

```text
cargo run -p rbus-bench -- --smoke
{"status":"ok","warning_count":0,"scenario_count":10,"transport_count":7,"payload_size_count":10,"process_id":1,"pinning_enabled":false,"publisher_count":1,"subscriber_count":1,"messages_per_second":0,"blocking_wait_wake_latency_ns":19795}
```

Smoke benchmark подтверждает, что blocking wait path проходит через kernel
`futex(FUTEX_WAIT/FUTEX_WAKE)` на topic-level shared segment-е без polling
`sleep`. Значение latency является smoke-метрикой конкретного контейнерного
прогона, а не стабильным SLA.

Полный suite создаёт новый domain rbus, запускает `rbusd`, затем поднимает
worker-процессы `publisher`, `subscriber`, `ping-responder`, `ping-requester` и
crash roles через hidden режим `--suite-worker`. Payload не проходит через
daemon; worker-ы общаются через обычный public API `Bus`/`Publisher`/`Subscriber`.

Обязательная матрица:

- payload: 64 B, 256 B, 1 KiB, 4 KiB, 16 KiB, 64 KiB, 256 KiB, 1 MiB, 4 MiB,
  8 MiB;
- topologies: 1/1, 1/2, 1/4, 2/1, 4/1, 4/4, ping-pong, slow subscriber,
  subscriber crash с удержанным `MessageRef`, publisher crash;
- wait modes: `busy-poll`, `recv`, `recv-timeout`;
- rbus transports: `rbus-loaned`, `rbus-publish-bytes`.

Suite report содержит p50/p90/p99/p99.9/p99.99 one-way latency, p50/p90/p99/p99.9
round-trip latency, throughput, payload throughput GiB/sec, CPU time, RSS/PSS,
context switches, allocations/message, syscalls/message, warning counters,
skipped messages, pinned-slot backpressure, startup latency и attach latency.

Быстрая проверка multi-process harness:

```text
cargo run -p rbus-bench -- --suite --matrix-smoke --messages 1 --format csv --no-gate
```

Baseline для regression gates находится в
`benches/baselines/rbus_suite_baseline.csv`. Gate проверяет p50/p99 latency,
throughput, allocations/message, syscalls/message и warning count. Portable
baseline в репозитории использует wildcard-строки; на reference host его нужно
заменить exact baseline-ами с тем же CSV-форматом после контрольного прогона.
