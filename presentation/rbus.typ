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
#let scheme-label(body) = text(size: body-size, weight: 400, fill: ink)[#body]
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
#let diagram-node(body, width: 3.2cm, height: 1.25cm) = box(
  width: width,
  height: height,
)[
  #align(center + horizon)[
    #text(size: body-size, weight: 800, fill: ink)[#body]
  ]
]
#let diagram-arrow(body) = align(center + horizon)[
  #text(size: body-size, weight: 800, fill: ink)[#body]
]
#let segment-block(title, body, width: 3.2cm) = box(
  width: width,
  height: 1.65cm,
)[
  #align(center)[
    #text(size: body-size, weight: 800, fill: ink)[#title]
    #v(0.12em)
    #text(size: body-size, fill: ink)[#body]
  ]
]
#let flow-width = 10.2cm
#let path-label(body, width: flow-width) = block(width: width)[
  #align(center)[
    #text(size: body-size, weight: 800, fill: ink)[#body]
  ]
]
#let flow-node(body, width: flow-width) = block(
  width: width,
)[
  #align(center)[
    #text(size: body-size, weight: 400, fill: ink)[#body]
  ]
]
#let flow-down = block(width: flow-width)[
  #v(0.16em)
  #align(center)[#scale(x: 72%, y: 72%, reflow: true)[#text(size: body-size, weight: 400, fill: ink)[↓]]]
  #v(0.16em)
]
#let memory-row(label, note, height: 0.9cm) = box(
  width: 11.8cm,
  height: height,
)[
  #align(center + horizon)[
    #text(size: body-size, weight: 400, fill: ink)[#label]
  ]
]
#let mapping-row(left, right) = grid(
  columns: (2.8cm, 0.6cm, 3.8cm),
  gutter: 0.15em,
  align: horizon,
  text(size: body-size, font: "IBM Plex Mono", fill: ink)[#left],
  text(size: body-size, weight: 400, fill: ink)[→],
  text(size: body-size, font: "IBM Plex Mono", fill: ink)[#right],
)

#title-slide[
  #v(0.65em)
  #text(size: title-size, weight: 850, fill: green)[rbus]
  #v(0.55em)
  #text(size: large-size, weight: 750, fill: ink)[локальная высокопроизводительная шина сообщений \
    для автомобильных и робототехнических систем]
]

== Проблема

#lead[Нужно передавать данные между процессами в рамках одного устройства]

#v(0.55em)

- Универсальные робототехнические фреймворки (по типу ROS 2) часто рассчитаны на широкий класс систем

- В них есть сетевые сценарии, разные режимы доставки (QoS-ы) и куча runtime-настроек

- Есть сценарии, где такая универсальность не требуется - например, когда процессы живут физически в одном месте

#v(0.7em)
#accent[Отсюда возникла идея сделать локальный транспорт, который и работает шустро, и не ест много ресурсов CPU]

== Цель проекта

#lead[Сделать такой транспорт]

Поскольку основным паттерном взаимодействия в такого рода системах является publish/subscribe, то основное внимание в работе будет уделено именно ему

#v(0.55em)

Что требуется:

- #accent[Производительность] — активное использование shared memory, loaned payload без копирования, блокирующий и неблокирующий пути передачи с минимальным количеством syscall-ов

- #accent[Простая модель и локальный scope] — простой и понятный API, одно устройство, один daemon, без попытки сделать универсальный middleware

- #accent[Надёжность] — освобождение ресурсов при падении/штатной остановки процессов, restart daemon-а не должен ломать уже настроенный путь для данных

- #accent[Простая и информативная диагностика] — cli-утилиты, stack trace-ы и т. д.

== Границы проекта

#lead[Что не входит в текущий этап]

- сетевой транспорт

- сложные политики доставки

- async/callback API

- serde-схемы

- C/Python bindings

- автоматический запуск daemon-а

== Архитектура

#lead[Daemon строит связи, payload идёт напрямую]

#v(0.55em)

#accent[Control plane]

#let arch-columns = (3.3cm, 0.9cm, 4.1cm, 0.9cm, 3.3cm)

#grid(
  columns: arch-columns,
  gutter: 0.2em,
  align: horizon,
  diagram-node([Publisher], width: 3.3cm),
  diagram-arrow[↔],
  diagram-node([rbusd], width: 4.1cm),
  diagram-arrow[↔],
  diagram-node([Subscriber], width: 3.3cm),
)

- REGISTER / ATTACH / INSPECT идут через rbusd по Unix SOCK_SEQPACKET

- FD передаются через SCM_RIGHTS

- daemon владеет registry и lifecycle

#v(0.45em)
#accent[Data plane]

#grid(
  columns: arch-columns,
  gutter: 0.2em,
  align: horizon,
  diagram-node([Publisher], width: 3.3cm),
  diagram-arrow[→],
  diagram-node([memfd\ MAP_SHARED], width: 4.1cm),
  diagram-arrow[→],
  diagram-node([Subscriber], width: 3.3cm),
)

- daemon не копирует и не маршрутизирует payload

- после attach hot path работает через shared memory

== Flow

#v(0.55em)

#grid(
  columns: (1fr, 1fr),
  gutter: 0.9em,
  [
    #align(center)[
      #block(width: flow-width)[
        #path-label[Publisher path]
        #v(0.28em)
        #grid(
          columns: 1fr,
          gutter: 0.10cm,
          flow-node[Bus::connect()],
          flow-down,
          flow-node[bus.publisher(topic, TopicConfig::new(max_size, depth))],
          flow-down,
          flow-node[rbus отправляет REGISTER_PUBLISHER в rbusd],
          flow-down,
          flow-node[rbusd создаёт memfd stream segment],
          flow-down,
          flow-node[rbusd делает ftruncate до нужного размера],
          flow-down,
          flow-node[rbusd инициализирует header/layout],
          flow-down,
          flow-node[rbusd передаёт FD publisher-у через SCM_RIGHTS],
          flow-down,
          flow-node[publisher mmap-ит segment],
        )
      ]
    ]
  ],
  [
    #align(center)[
      #block(width: flow-width)[
        #path-label[Subscriber path]
        #v(0.28em)
        #grid(
          columns: 1fr,
          gutter: 0.10cm,
          flow-node[bus.subscriber(topic)],
          flow-down,
          flow-node[rbus отправляет REGISTER_SUBSCRIBER],
          flow-down,
          flow-node[rbusd выдаёт subscriber_id и topic wait FD],
          flow-down,
          flow-node[subscriber вызывает ATTACH_STREAMS],
          flow-down,
          flow-node[rbusd передаёт FD всех stream-ов этого topic-а],
          flow-down,
          flow-node[subscriber mmap-ит эти stream segment-ы],
        )
      ]
    ]
  ],
)

== Ring в shared memory

#lead[Ring — фиксированный массив slot-ов, который используется циклически]

#v(0.35em)

#align(center)[
  #scale(x: 65%, y: 65%, reflow: true)[
    #box(width: 25cm)[
      ```text
      ┌──────────────────────┐
      │ SegmentHeader        │  общая информация о stream-е
      ├──────────────────────┤
      │ StreamCounters       │  статистика
      ├──────────────────────┤
      │ SlotHeader[0]        │  metadata slot-а 0
      │ SlotHeader[1]        │  metadata slot-а 1
      │ ...                  │
      │ SlotHeader[63]       │  metadata slot-а 63
      ├──────────────────────┤
      │ PayloadSlot[0]       │  bytes сообщения для slot-а 0
      │ PayloadSlot[1]       │  bytes сообщения для slot-а 1
      │ ...                  │
      │ PayloadSlot[63]      │  bytes сообщения для slot-а 63
      └──────────────────────┘
      ```
    ]
  ]
]

#v(0.18em)

Publisher пишет payload не "просто в stream", а в конкретный payload slot внутри stream-а:

#v(0.08em)

#align(center)[
  #scale(x: 72%, y: 72%, reflow: true)[
    ```text
    сообщение #0  -> PayloadSlot[0]
    сообщение #1  -> PayloadSlot[1]
    сообщение #2  -> PayloadSlot[2]
    ...
    сообщение #63 -> PayloadSlot[63]
    сообщение #64 -> PayloadSlot[0] снова
    сообщение #65 -> PayloadSlot[1] снова
    ```
  ]
]

#v(0.08em)

Поэтому это называется ring: после последней ячейки возвращаемся в начало.

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

== Что сравниваем

#lead[Unix socket baseline против rbus SHM data path]

#v(0.55em)

- #accent[Одинаковый scope] — локальный IPC на одной Linux-машине
- #accent[Unix socket] — payload проходит через kernel socket path и socket buffer
- #accent[rbus] — payload лежит в shared-memory ring, loaned path не копирует payload
- #accent[Что проверяем] — что даёт переход от socket baseline к SHM data path

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
