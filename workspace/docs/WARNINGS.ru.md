# Каталог предупреждений

Все штатные сбои rbus возвращаются как `Warning`. Публичного `RbusError` нет.

Коды предупреждений поддерживаются в `rbus-core::warning::WarningCode`; каждый код
имеет русское описание и тест на наличие описания.

`Warning` хранит код, компонент, операцию и compact context без heap allocation.
Русский текст вычисляется из `WarningCode`, а optional context-поля хранятся через
bitmask. Это сохраняет публичный тип `RbusResult<T> = Result<T, Warning>` без
boxing и без clippy warning `result_large_err`.

На syscall boundary `rbus-ipc` возвращает `Warning`:

- `SCM_RIGHTS` send/receive: `SyscallFailed`, `ResourceLimit` или `ProtocolMismatch`;
- подключение CLI к Unix socket daemon-а: `DaemonUnavailable`;
- framed control IO: `Disconnected`, `SyscallFailed`, `ResourceLimit` или `ProtocolMismatch`;
- `futex` wait/wake: `SyscallFailed` или `ResourceLimit`;
- `memfd`, `mmap`, `eventfd`, `clock_gettime`: профильные syscall warning-и.

Fallback warning path для `shm_open` отсутствует, потому что сам fallback не
реализуется в текущем runtime.
