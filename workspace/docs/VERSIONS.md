# Версии API/ABI

Текущий контракт версий фиксируется в `rbus-core::constants` и проверяется
тестом `api_and_abi_versions_are_centrally_synced`

| Контракт | Версия | Константа | Когда менять |
| --- | --- | --- | --- |
| Публичный API | `0.1.0` | `RBUS_API_VERSION` | При изменении публичного API crate-а или версии пакета workspace |
| Управляющий протокол | `2` | `CONTROL_PROTOCOL_VERSION` | При изменении layout frame-а, набора opcode, схемы payload или семантики handshake |
| Stream segment ABI | `5` | `STREAM_SEGMENT_ABI_VERSION` | При изменении `SegmentHeader`, `StreamCounters`, `SlotHeader`, смещений, flags или layout payload |
| Topic wait segment ABI | `1` | `TOPIC_WAIT_ABI_VERSION` | При изменении `TopicWaitHeader` или видимой через futex семантики ожидания |

`SHARED_MEMORY_ABI_VERSION` и `SHARED_MEMORY_MAGIC` оставлены только как alias
для stream segment ABI. Новый код должен использовать
`STREAM_SEGMENT_ABI_VERSION` и `STREAM_SEGMENT_MAGIC`, чтобы не смешивать stream
segment с topic-wait segment

`rbus-cli doctor --domain <domain>` выводит эти версии отдельными полями:
`api_version`, `control_protocol_version`, `stream_segment_abi_version` и
`topic_wait_abi_version`. Старое имя `shm_abi` не используется, потому что оно
не различает stream segment ABI и topic wait segment ABI

Правило bump-а строгое: если новый бинарь не может безопасно подключить старый
segment или корректно прочитать старый управляющий frame, соответствующая ABI
или версия протокола должна измениться. Резервный путь на старый layout
запрещён; несовпадение завершается `WarningCode::SegmentAbiMismatch` или
`WarningCode::ProtocolMismatch`, а CLI/daemon/benchmark при аварийном
завершении печатают stacktrace
