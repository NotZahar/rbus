#import "@preview/touying:0.7.4": *
#import themes.simple: *

#let ink = rgb("#17202a")
#let green = rgb("#16635c")
#let white = rgb("#ffffff")

#let title-size = 24pt
#let large-size = 19pt
#let body-size = 16pt
#let slide-margin = 1.2em

#let slide-title = block(below: 1.05em)[
  #text(size: title-size, weight: 800, fill: ink)[
    #utils.display-current-heading(
      level: 2,
      style: (setting: body => body, numbered: true, current-heading) => current-heading.body,
    )
  ]
]

#let page-number-layer = context {
  let current = utils.slide-counter.get().first()
  if current > 0 {
    place(
      bottom + right,
      dx: -slide-margin,
      dy: -slide-margin,
      text(size: body-size, fill: ink, weight: 700)[#utils.slide-counter.display() / #utils.last-slide-number],
    )
  }
}

#show: simple-theme.with(
  aspect-ratio: "16-9",
  header: none,
  footer: none,
  footer-right: none,
  primary: green,
  subslide-preamble: slide-title,
  config-page(margin: slide-margin, foreground: page-number-layer),
  config-methods(
    init: (self: none, body) => {
      set text(font: "Noto Sans", lang: "ru", size: body-size, fill: ink)
      show heading.where(level: 1): set text(font: "Noto Sans", size: title-size, weight: 800, fill: ink)
      show heading.where(level: 2): set text(font: "Noto Sans", size: title-size, weight: 800, fill: ink)
      show raw: set text(font: "IBM Plex Mono", size: body-size, fill: ink)
      body
    },
  ),
)

#set par(leading: 0.5em)
#set list(indent: 0.8em, body-indent: 0.55em)

#let lead(body) = text(size: body-size, weight: 800, fill: ink)[#body]
#let accent(body) = text(size: body-size, weight: 750, fill: green)[#body]
#let callout(title, body) = [
  #accent(title)
  #v(0.18em)
  #body
]
#let stat(value, label) = [
  #text(size: large-size, weight: 800, fill: green)[#value]
  #v(0.15em)
  #text(size: body-size, fill: ink)[#label]
]
#let bar(label, value, width) = grid(
  columns: (4.9cm, 1fr, 2.3cm),
  gutter: 0.55em,
  align: (left, left, right),
  text(size: body-size)[#label],
  box(width: width, height: 0.24cm, fill: green),
  text(size: body-size, weight: 750)[#value],
)
#let plain-table(cols, body) = [
  #set text(size: body-size)
  #table(
    columns: cols,
    stroke: none,
    inset: (x: 5pt, y: 5pt),
    align: (left, left, left, left, left),
    ..body,
  )
]
#let result-row(a, b, c, d, e, header: false) = {
  let weight = if header { 750 } else { 400 }
  let fill = if header { green } else { ink }
  grid(
    columns: (2.2cm, 3.0cm, 3.0cm, 3.0cm, 2.3cm),
    gutter: 0.9em,
    text(size: body-size, weight: weight, fill: fill)[#a],
    text(size: body-size, weight: weight, fill: fill)[#b],
    text(size: body-size, weight: weight, fill: fill)[#c],
    text(size: body-size, weight: weight, fill: fill)[#d],
    text(size: body-size, weight: weight, fill: fill)[#e],
  )
}

#title-slide[
  #v(1.2em)
  #text(size: title-size, weight: 850, fill: green)[rbus]
  #v(0.55em)
  #text(size: large-size, weight: 750, fill: ink)[локальная высокопроизводительная шина сообщений \
    для автомобильных и робототехнических систем]
]

== Проблема

#lead[Нужно быстро передавать данные между процессами на одной машине]

#v(0.55em)

- Универсальные robotics-фреймворки часто рассчитаны на широкий класс систем
- В них есть сетевые сценарии, разные режимы доставки и много runtime-настроек
- В наших задачах все процессы работают на одном Linux-устройстве
- Поэтому главный вопрос проще: как сделать быстрый локальный IPC без лишних копий

#v(0.7em)
#accent[rbus не заменяет универсальный middleware целиком]

Это локальный IPC/SHM-транспорт для среза, где участники находятся на одном устройстве

== Цели проекта

#lead[Сделать локальный fast path для publish/subscribe]

#v(0.55em)

- #accent[Производительность] — shared memory, loaned payload без копирования, non-blocking hot path без syscall и heap allocation
- #accent[Простая модель и локальный scope] — минимальный Rust API, одна машина, один daemon registry, без попытки заменить универсальный middleware
- #accent[Надёжность] — cleanup при падении процессов, closed stream через shared memory, restart daemon-а не ломает уже attached data plane
- #accent[Диагностика] — rbus-cli, doctor, stats, warnings, inspect, benchmark smoke

== Требования

#lead[Что должен уметь текущий этап]

#v(0.55em)

#accent[Функциональные]

- несколько publisher-ов и subscriber-ов на topic
- fan-out доставки одному и нескольким consumer-ам
- blocking и non-blocking receive
- контрольный daemon и CLI-диагностика
- cleanup после смерти publisher/subscriber

#v(0.45em)
#accent[Нефункциональные]

- Linux-only, локальная машина
- payload не проходит через daemon
- 0 syscalls/message для non-blocking data path
- 0 allocations/message в zero-copy hot path
- ABI/version contracts и воспроизводимые бенчмарки

== Границы проекта

#lead[Что входит в текущий этап]

#v(0.55em)

Локальный publish/subscribe, shared-memory ring, FD passing, daemon registry, warning model, CLI, benchmark harness

#v(0.75em)
#lead[Что не входит]

Сетевой транспорт, сложные политики доставки, async/callback API, serde-схемы, C/Python bindings, автоматический запуск daemon-а

== Что сравниваем

#lead[Unix socket baseline против rbus SHM data path]

#v(0.55em)

- #accent[Одинаковый scope] — локальный IPC на одной Linux-машине
- #accent[Unix socket] — payload проходит через kernel socket path и socket buffer
- #accent[rbus] — payload лежит в shared-memory ring, loaned path не копирует payload
- #accent[Что проверяем] — что даёт переход от socket baseline к SHM data path

== Архитектура: общий вид

#lead[Daemon управляет ресурсами, payload идёт напрямую]

#v(0.55em)

#accent[Control plane]

Publisher process ↔ rbusd ↔ Subscriber process

- REGISTER / ATTACH / INSPECT через Unix SOCK_SEQPACKET
- FD передаются через SCM_RIGHTS
- daemon владеет registry и lifecycle

#v(0.65em)
#accent[Data plane]

Publisher stream memfd/mmap MAP_SHARED → Subscriber cached pointers

- daemon не копирует payload
- daemon не маршрутизирует payload
- после attach hot path работает через shared memory

== Архитектура: control plane

#lead[rbusd нужен для setup, registry и cleanup]

#v(0.55em)

- #accent[REGISTER_PUBLISHER] создаёт memfd stream segment и выдаёт publisher_id, stream_id, FD
- #accent[REGISTER_SUBSCRIBER] выдаёт subscriber_id и metadata для attach
- #accent[ATTACH_STREAMS] передаёт stream FD через SCM_RIGHTS
- #accent[NOTIFY_STREAM_ADDED] добавляет новые publisher stream-ы без refresh в steady state
- persistent control session связывает registry entries с процессом

== Архитектура: data plane

#lead[Один publisher stream — это fixed-size SPMC ring]

#v(0.55em)

#accent[Memory layout]

- SegmentHeader
- StreamCounters
- SlotHeader[depth]
- PayloadSlot[depth]

#v(0.5em)
#accent[Slot protocol]

- writer получает slot и пишет payload
- commit публикуется release-store в state_seq
- reader pin-ит slot acquire/release CAS-ом
- MessageRef освобождает reader bit при Drop
- blocking recv ждёт topic-level futex segment

== Публичный API

#lead[Минимальные типы, без лишней runtime-модели]

#v(0.55em)

- #accent[Типы] — Bus, TopicConfig, Publisher, Subscriber, LoanedMessage, MessageRef, Stats, Warning
- #accent[Сценарий] — connect → publisher/subscriber → loan → write payload → publish → receive
- #accent[Контракты] — public API 0.1.0, stream ABI 5, topic wait ABI 1

== Гарантии hot path

#lead[Главные обещания data path]

#v(0.55em)

#grid(
  columns: (1fr, 1fr, 1fr),
  gutter: 0.9em,
  stat([0], [heap allocations: loan + publish]),
  stat([0], [syscalls/message: non-blocking publish/try_recv]),
  stat([0], [payload copies в loaned path]),
)

#v(0.75em)
#accent[Как проверяется]

- custom allocator tests
- marker-based strace smoke
- Loom-модели atomic protocol
- layout tests на cache-line counters

== Технологии

#lead[Минимальный Linux/Rust стек]

#v(0.55em)

- #accent[Rust 2024] — workspace package 0.1.0, warnings=deny, dead_code=deny
- #accent[Linux IPC] — memfd_create, ftruncate, mmap MAP_SHARED, Unix SOCK_SEQPACKET, SCM_RIGHTS
- #accent[Синхронизация] — futex для blocking recv, acquire/release atomics для slot protocol
- #accent[Benchmarking] — perf stat, strace, CSV/JSON reports, regression gates

== Методика измерений

#lead[Окружение и режимы нагрузки]

#v(0.55em)

- Ubuntu 26.04 LTS, Linux 7.0.0-27-generic
- Intel Core i7-11700F: 8C/16T
- governor: powersave, intel_pstate active
- cargo build --profile bench
- rbusd и rbus-bench из target/release

#v(0.55em)
#accent[Режимы]

- saturated / busy-poll: 4 publisher-а, 8 subscriber-ов
- payload: 64 B, 4 KiB, 64 KiB, 1 MiB
- paced / 30 Hz: controlled-load latency
- low-CPU recv: 5 секунд, 150 сообщений, 1P1S

== Результаты: throughput

#lead[Saturated busy-poll, 4P8S, payload 64 B]

#v(0.55em)

#bar([rbus loaned-fill], [25.54M msg/s], 100%)
#bar([rbus publish-bytes], [24.15M msg/s], 94.6%)
#bar([unix-socket], [4.50M msg/s], 17.6%)

#v(0.75em)
#grid(
  columns: (1fr, 1fr, 1fr),
  gutter: 0.9em,
  stat([5.7x], [loaned-fill vs unix socket]),
  stat([0], [syscalls/message в rbus busy-poll data path]),
  stat([0], [allocations/message в rbus busy-poll hot path]),
)

== Результаты: большие payload

#lead[Saturated busy-poll, effective delivery GiB/s]

#v(0.55em)

#result-row([Payload], [loaned], [publish], [unix], [x], header: true)
#v(0.15em)
#result-row([4 KiB], [53.8], [59.1], [8.2], [6.5x])
#result-row([64 KiB], [120.2], [92.1], [21.8], [5.5x])
#result-row([1 MiB], [146.1], [154.1], [15.6], [9.4x])

#v(0.75em)
#accent[metadata-only]

1 MiB: 661.2 GiB/s logical/effective

Это commit/control overhead без записи payload в measured window

== Результаты: задержка под нагрузкой

#lead[Paced 30 Hz, 1P1S, busy-poll, p99 transport latency]

#v(0.55em)

#result-row([Payload], [loaned], [publish], [unix], [x], header: true)
#v(0.15em)
#result-row([64 B], [1.2 us], [2.4 us], [126.7 us], [105x])
#result-row([4 KiB], [8.0 us], [9.2 us], [151.7 us], [19x])
#result-row([64 KiB], [23.5 us], [23.8 us], [276.1 us], [12x])
#result-row([1 MiB], [23.2 us], [110.2 us], [1615.5 us], [70x])

#v(0.75em)
- Этот режим показывает нижнюю границу latency
- Он использует busy-poll и почти целое ядро на worker path
- Metadata-only не используется как основной latency-result

== Результаты: низкий CPU в recv

#lead[Paced 30 Hz, 1P1S, wait-mode recv]

#v(0.55em)

#accent[64 B]

- rbus loaned-fill: p99 81 us, 0.004 cores
- unix-socket: p99 103 us, 0.001 cores
- вывод: rbus быстрее по latency, Unix дешевле по CPU

#v(0.55em)
#accent[1 MiB]

- rbus loaned-fill: p99 231 us, 0.014 cores
- unix-socket: p99 1369 us, 0.019 cores
- вывод: rbus ниже по latency при сопоставимом CPU

== Результаты: корректность

#lead[Benchmark-и не должны скрывать ошибки доставки]

#v(0.55em)

#grid(
  columns: (1fr, 1fr, 1fr),
  gutter: 0.9em,
  stat([0], [warning_count в выбранных строках]), stat([0], [skipped_messages]), stat([0], [publish deadline misses]),
)

#v(0.75em)
#accent[Тесты]

- cross-process data plane tests
- examples suite
- subscriber crash с удержанным MessageRef
- publisher crash
- rbusd restart

== Сильные стороны и ограничения

#lead[Что получилось и где границы]

#v(0.55em)

#accent[Сильные стороны]

- payload не проходит через daemon
- zero-copy loaned path
- высокий throughput на локальной машине
- microsecond-level paced latency в busy-poll
- низкий CPU recv path через futex

#v(0.55em)
#accent[Ограничения]

- Linux-specific IPC primitives
- API пока Rust-only
- нет сетевого транспорта
- один stream ограничен 63 subscriber reader bits
- low-CPU small payload не всегда дешевле Unix socket по CPU

== Выводы

#lead[rbus закрывает локальный IPC-срез проекта]

#v(0.55em)

- #accent[Цель достигнута] — реализован локальный message bus, где setup делает daemon, а payload идёт через shared memory
- #accent[Ниша понятна] — fast local transport для процессов на одной машине, не замена универсального middleware
- #accent[Результат измерен] — есть throughput, latency, perf/strace данные и regression tests

#v(0.75em)
#lead[Следующие шаги]

- adapter для внешнего robotics stack
- стабилизация API
- C++ binding для robotics-кода
- реальные sensor payloads
- долгие soak-тесты
