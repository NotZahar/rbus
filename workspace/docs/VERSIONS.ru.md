# Версии API/ABI

Текущий контракт версий фиксируется в `rbus-core::constants` и проверяется
тестом `api_and_abi_versions_are_centrally_synced`.

| Контракт | Версия | Константа | Когда менять |
| --- | --- | --- | --- |
| Public API | `0.1.0` | `RBUS_API_VERSION` | При изменении публичного crate API или workspace package version. |
| Control protocol | `2` | `CONTROL_PROTOCOL_VERSION` | При изменении frame layout, opcode set, payload schema или handshake semantics. |
| Stream segment ABI | `4` | `STREAM_SEGMENT_ABI_VERSION` | При изменении `SegmentHeader`, `StreamCounters`, `SlotHeader`, offsets, flags или payload layout. |
| Topic wait segment ABI | `1` | `TOPIC_WAIT_ABI_VERSION` | При изменении `TopicWaitHeader` или futex-visible wait semantics. |

`SHARED_MEMORY_ABI_VERSION` и `SHARED_MEMORY_MAGIC` оставлены только как alias
для stream segment ABI. Новый код должен использовать
`STREAM_SEGMENT_ABI_VERSION` и `STREAM_SEGMENT_MAGIC`, чтобы не смешивать stream
segment с topic-wait segment.

`rbus-cli doctor --domain <domain>` выводит эти версии отдельными полями:
`api_version`, `control_protocol_version`, `stream_segment_abi_version` и
`topic_wait_abi_version`. Старое имя `shm_abi` не используется, потому что оно
не различает stream segment ABI и topic wait segment ABI.

Правило bump-а строгое: если новый бинарь не может безопасно attach-ить старый
segment или корректно прочитать старый control frame, соответствующая ABI или
protocol version должна измениться. Fallback на старый layout запрещён; mismatch
завершается `WarningCode::SegmentAbiMismatch` или
`WarningCode::ProtocolMismatch` с stacktrace на fatal path.
