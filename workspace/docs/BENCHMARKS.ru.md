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

У benchmark suite есть два режима нагрузки:

- `--load-mode saturated` — режим по умолчанию и прежнее поведение. Publisher-ы
  отправляют сообщения настолько быстро, насколько позволяет transport. Основная
  метрика — максимальный throughput, а latency в этом режиме включает queueing
  latency насыщенного ring/buffer-а.
- `--load-mode paced` — режим controlled-load latency. В первом срезе
  поддерживается только `--rate-hz 30` и `--rate-scope per-publisher`, то есть
  каждый publisher публикует 30 сообщений в секунду. Количество сообщений
  вычисляется как `floor(duration_seconds * rate_hz)` из `--duration-ms`;
  одновременная передача `--messages` и `--duration-ms` в paced mode считается
  ошибкой конфигурации.

Paced smoke:

```text
target/release/rbus-bench --smoke --load-mode paced --rate-hz 30 --duration-ms 1000
```

Актуальная saturated suite matrix намеренно ограничена; сам suite имеет hard timeout 20
минут и завершает прогон ошибкой `Timeout`, если бюджет исчерпан перед очередным
измерением:

- topology: 4 publisher-а и 8 subscriber-ов, то есть 2 subscriber-а на каждого
  publisher-а;
- wait mode: `busy-poll`;
- payload: 64 B, 256 B, 1 KiB, 4 KiB, 16 KiB, 64 KiB, 256 KiB, 1 MiB, 4 MiB,
  8 MiB;
- transports: `rbus-loaned-metadata-only`, `rbus-loaned-fill`,
  `rbus-publish-bytes`, `unix-socket`, `unix-socket-end-to-end`.

Первый paced-срез использует отдельную минимальную матрицу:

- topology: `one-publisher-one-subscriber`, `one-publisher-four-subscribers`,
  `four-publishers-eight-subscribers`;
- wait mode: `busy-poll`;
- payload: 64 B, 4 KiB, 64 KiB, 1 MiB, 8 MiB;
- rate: 30 Hz на publisher;
- transports: `rbus-loaned-metadata-only`, `rbus-loaned-fill`,
  `rbus-publish-bytes`, `unix-socket`.

Перед запуском paced строки harness считает configured write/delivery GiB/s и
падает до старта worker-ов, если workload превышает safety cap. Rate или
duration не clamp-ятся.

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
parent создаёт общий start flag с monotonic timestamp немного в будущем.
Worker-ы входят в measured window по этому абсолютному времени; fixed sleep для
attach readiness не используется.

В paced mode publisher использует абсолютное расписание, а не `sleep(period)`
после publish:

```text
deadline = benchmark_start + message_index * period
wait_until(deadline)
publish_one_message()
```

`--pace-spin-tail-us` по умолчанию равен 0. Если он больше 0, worker sleep-ит до
`deadline - spin_tail`, затем busy-spin до deadline. Это поведение benchmark
driver-а и не входит в `publish_syscalls/message` или `try_recv_syscalls/message`.

Suite report содержит две one-way latency-семантики:

- `transport_latency_*_ns`: timestamp ставится после подготовки payload;
- `end_to_end_latency_*_ns`: timestamp ставится перед генерацией payload.

Для paced latency основной сигнал — `transport_latency_*_ns`: payload fill или
подготовка baseline buffer-а уже завершены, timestamp ставится прямо перед
commit/write. `end_to_end_latency_*_ns` показывает путь от начала генерации
payload до receive и нужен отдельно, чтобы не смешивать эти две семантики.

Paced report дополнительно содержит:

- `publish_lateness_*_ns` и `publish_deadline_miss_count` — качество соблюдения
  абсолютного расписания;
- `inter_publish_gap_*_ns` и `inter_receive_gap_*_ns` — jitter публикации и
  доставки;
- `backlog_metric_available` и `ring_backlog_*_messages` — occupancy/backlog.
  Для rbus backlog считается как агрегированное отставание subscriber-а от
  опубликованных сообщений; для baseline transport-ов поле доступности равно
  `false`;
- `unique_publish_messages_per_second` — фактическая уникальная публикация;
- `delivered_messages_per_second` — фактическая fan-out доставка;
- `actual_payload_write_gib_per_second` — объём уникально записанного payload;
- `effective_payload_delivery_gib_per_second` — fan-out payload delivery. Это не
  физическая memory bandwidth.

Для `rbus-loaned-metadata-only` GiB/s является logical/effective метрикой:
payload slot предварительно подготовлен, а в measured window обновляются только
timestamp/metadata и commit.

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
`load_mode`, `payload_size`, `publisher_count`, `subscriber_count`,
`measured_messages`.
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

`bench:reference` запускает suite с `--no-gate` и записывает raw measured CSV
baseline без ослабляющих threshold-ов. `bench:perf` запускает suite с regression
gate против baseline: latency допускает максимум 5% регрессии, а allocations,
syscalls, warning count, skipped messages и pinned-slot backpressure проверяются
как ceiling/hard-zero по baseline. Для короткой проверки можно добавить
`--matrix-smoke`.
