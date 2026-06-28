use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use rbus_core::warning::{Warning, WarningCode};

use crate::worker::WorkerArguments;

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const HASH_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const HASH_PRIME: u64 = 0x0000_0100_0000_01B3;

pub(crate) fn stream_path(
    arguments: &WorkerArguments,
    topic: &str,
    extension: &str,
    subscriber_index: u8,
) -> PathBuf {
    std::env::temp_dir().join(format!(
        "rb-{}-{subscriber_index}.{extension}",
        stable_hash(arguments, topic)
    ))
}

pub(crate) fn pair_path(
    arguments: &WorkerArguments,
    topic: &str,
    extension: &str,
    publisher_index: u8,
    subscriber_index: u8,
) -> PathBuf {
    std::env::temp_dir().join(format!(
        "rb-{}-{publisher_index}-{subscriber_index}.{extension}",
        stable_hash(arguments, topic)
    ))
}

pub(crate) fn queue_name(
    arguments: &WorkerArguments,
    topic: &str,
    publisher_index: u8,
    subscriber_index: u8,
) -> Result<CString, Warning> {
    CString::new(format!(
        "/rb{}{}{}",
        stable_hash(arguments, topic),
        publisher_index,
        subscriber_index
    ))
    .map_err(|_| invalid_argument())
}

pub(crate) fn make_fifo(path: &Path) -> Result<(), Warning> {
    let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|_| invalid_argument())?;
    // SAFETY: c_path is nul-terminated and points to a valid filesystem path for this call.
    let result = unsafe { libc::mkfifo(c_path.as_ptr(), libc::S_IRUSR | libc::S_IWUSR) };
    if result != 0 {
        return Err(warning_from_errno("mkfifo"));
    }
    Ok(())
}

pub(crate) fn remove_path(path: &Path) -> Result<(), Warning> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(warning_from_io(error)),
    }
}

fn stable_hash(arguments: &WorkerArguments, topic: &str) -> u64 {
    let mut hash = HASH_OFFSET_BASIS;
    for byte in arguments
        .domain
        .as_bytes()
        .iter()
        .chain(topic.as_bytes())
        .chain(arguments.transport.as_str().as_bytes())
    {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(HASH_PRIME);
    }
    hash
}

fn current_errno() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EINVAL)
}

fn warning_from_io(error: std::io::Error) -> Warning {
    match error.raw_os_error() {
        Some(errno) => Warning::new(
            WarningCode::SyscallFailed,
            COMPONENT_RBUS_BENCH,
            "baseline_ipc",
        )
        .with_os_errno(errno),
        None => Warning::new(
            WarningCode::SyscallFailed,
            COMPONENT_RBUS_BENCH,
            "baseline_ipc",
        ),
    }
}

fn warning_from_errno(operation: &'static str) -> Warning {
    Warning::new(WarningCode::SyscallFailed, COMPONENT_RBUS_BENCH, operation)
        .with_os_errno(current_errno())
}

fn invalid_argument() -> Warning {
    Warning::new(
        WarningCode::InvalidConfig,
        COMPONENT_RBUS_BENCH,
        "baseline_ipc",
    )
}
