# Производительность

Steady-state требования:

- loan + publish loaned: 0 heap allocations;
- try_recv при наличии сообщения: 0 heap allocations;
- try_recv без сообщения: 0 heap allocations;
- non-blocking publish/try_recv: 0 syscalls;
- payload copies in loaned path: 0.

Текущий `rbus` stream backend создаёт `memfd` и `mmap MAP_SHARED` на setup path.
После создания stream-а `loan`, `publish`, `try_recv` и `MessageRef::drop`
работают через atomics и pointer offsets внутри уже mapped segment-а.
`Publisher::publish` также записывает monotonic timestamp в `SlotHeader` перед
commit. Этот timestamp path проверяется тем же syscall smoke: если получение
времени начнёт выполнять traced syscall между hot-path marker-ами, проверка
падает.

0 heap allocations в zero-copy hot path доказываются integration test-ом
`zero_copy_hot_path_does_not_allocate`. Тестовый бинарь устанавливает custom
global allocator, считает `alloc`, `alloc_zeroed` и `realloc`, прогревает setup
path вне измерения и затем проверяет три операции:

- `Publisher::loan` + `Publisher::publish`;
- `Subscriber::try_recv` при наличии сообщения;
- `Subscriber::try_recv` без сообщения на уже attached stream-е.

Blocking `recv`/`recv_timeout` не используют polling `sleep`. Subscriber ждёт
topic-level futex word в отдельном shared segment-е. Publisher после commit
читает `wake_waiters_mask`; при нулевой маске publish не пишет `wake_epoch` и не
делает `FUTEX_WAKE`. Если blocking subscriber уже выставил waiter bit, publisher
инкрементирует topic wake epoch и делает `FUTEX_WAKE`. Перед уходом в futex wait
subscriber повторно проверяет `stream_epoch` и write sequence attached stream-ов,
поэтому publish между последним `try_recv` и постановкой waiter-а не теряется.

0 syscalls в non-blocking hot path доказываются TypeScript strace regression-скриптом
`/workspace/scripts/hot-path-syscall-smoke.ts`, который запускается через:

```bash
npm run hot-path:syscalls
```

Probe `rbus-bench --hot-path-syscall-probe publish` прогревает daemon setup,
registration, mmap attach и non-blocking subscriber attach до marker-а
`RBUS_HOT_PATH_BEGIN:publish`. Между BEGIN/END выполняется только steady-state
`Publisher::loan` + `Publisher::publish`; `strace` не должен видеть ни одного
syscall-а.

Probe `rbus-bench --hot-path-syscall-probe try-recv` заранее публикует сообщения,
attach-ит stream и прогревает cursor. Между marker-ами выполняется только
steady-state `Subscriber::try_recv` на уже attached stream-е; любые syscall-и
между marker-ами считаются регрессией.

Benchmark suite дополнительно измеряет `syscalls/message` для полной
multi-process matrix. Это отдельный `strace -c` pass после основного timing pass,
поэтому latency и throughput regression gate не искажаются overhead-ом трассировки,
а syscall budget проверяется измерением реальных worker-процессов.
