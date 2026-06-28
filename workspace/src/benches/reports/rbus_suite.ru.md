# Rbus Benchmark Suite

Полный suite запускает реальные процессы `rbusd`, publishers и subscribers для
всей матрицы payload/topology/wait mode. JSON/CSV вывод формируется процессом
родителем после агрегации stdout worker-ов.
По умолчанию каждая строка matrix выполняется пять раз, а latency/throughput и
resource metrics попадают в report как медиана независимых запусков. Syscall
pass выполняется один раз на строку matrix. Перед измерением worker-ы прогревают
transport 64 сообщениями на publisher.

Команда короткой проверки:

```text
cargo run -p rbus-bench -- --suite --matrix-smoke --messages 1 --format csv --no-gate
```

Regression gate по умолчанию читает
`benches/baselines/rbus_suite_baseline_x86_64.csv` и проверяет p50/p90/p99
latency, round-trip latency, throughput, payload throughput,
allocations/message, syscalls/message, warning count, skipped messages и
pinned-slot backpressure.

Метрика `syscalls/message` заполняется отдельным `strace -c` прогоном той же
топологии после основного timing pass. Основной прогон остаётся без трассировки,
поэтому latency и throughput не включают overhead `strace`.
Для `rbus` + `busy-poll` в suite используется 0 `syscalls/message`; этот hot
path отдельно проверяется marker-based strace smoke-тестом.

## Paced latency results

Paced mode запускается отдельно от saturated throughput:

```text
target/release/rbus-bench --smoke --load-mode paced --rate-hz 30 --duration-ms 1000
```

В первом срезе поддерживается только 30 Hz на publisher
(`rate_scope=per-publisher`). Каждая строка paced report отвечает на четыре
вопроса:

- выдержана ли частота: `unique_publish_messages_per_second` должен быть в
  пределах 1% от `publisher_count * rate_hz`;
- есть ли backlog: для rbus доступны `ring_backlog_p50/p90/p99/max_messages`,
  для baseline transport-ов `backlog_metric_available=false`;
- какая latency без payload fill: основной показатель —
  `transport_latency_p50_ns` и `transport_latency_p99_ns`;
- есть ли ошибки исполнения: `warning_count`, `skipped_messages` и
  `backpressure_pinned_slot` должны быть 0.

Для payload >= 4 KiB строка `rbus-loaned-fill` должна сравниваться с
`unix-socket` по `transport_latency_p50_ns`. Если rbus не быстрее, это
performance warning/failing goal, а не повод расширять baseline threshold.

`actual_payload_write_gib_per_second` показывает уникальную запись payload
publisher-ами. `effective_payload_delivery_gib_per_second` показывает fan-out
доставку subscriber-ам и не является физической memory bandwidth. Для
`rbus-loaned-metadata-only` GiB/s является logical/effective метрикой, потому что
payload не переписывается целиком в measured window.
