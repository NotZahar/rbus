# Rbus Benchmark Suite

Полный suite запускает реальные процессы `rbusd`, publishers и subscribers для
всей матрицы payload/topology/wait mode. JSON/CSV вывод формируется процессом
родителем после агрегации stdout worker-ов.

Команда короткой проверки:

```text
cargo run -p rbus-bench -- --suite --matrix-smoke --messages 1 --format csv --no-gate
```

Regression gate по умолчанию читает
`benches/baselines/rbus_suite_baseline.csv` и проверяет p50/p99 latency,
throughput, allocations/message, syscalls/message и warning count.
