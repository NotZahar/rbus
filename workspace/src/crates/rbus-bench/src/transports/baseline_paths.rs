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
