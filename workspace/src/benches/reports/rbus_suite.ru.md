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
