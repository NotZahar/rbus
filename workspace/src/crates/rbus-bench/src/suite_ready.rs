use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::Duration;

use rbus_core::warning::{Warning, WarningCode};

use crate::system_metrics::monotonic_time_ns;

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const READY_WAIT_ATTEMPTS: usize = 300_000;
const READY_WAIT_POLL_INTERVAL_MS: u64 = 1;
const START_DELAY_NS: u64 = 20_000_000;

pub(crate) fn ready_directory(run_index: usize) -> PathBuf {
    std::env::temp_dir().join(format!("rbus-bench-{}-{run_index}", std::process::id()))
}

pub(crate) fn attach_ready_file(directory: &Path, index: u8) -> PathBuf {
    named_ready_file(directory, "attach-subscriber", index)
}

pub(crate) fn worker_ready_file(directory: &Path, role: &str, index: u8) -> PathBuf {
    named_ready_file(directory, role, index)
}

pub(crate) fn start_file(directory: &Path) -> PathBuf {
    directory.join("start.flag")
}

pub(crate) fn warmup_start_file(directory: &Path) -> PathBuf {
    directory.join("warmup-start.flag")
}

pub(crate) fn write_start_file(directory: &Path) -> Result<(), Warning> {
    let start_ns = monotonic_time_ns()?
        .checked_add(START_DELAY_NS)
        .ok_or_else(|| warning("start_file::overflow"))?;
    std::fs::write(start_file(directory), format!("{start_ns}\n"))
        .map_err(|error| warning_from_io(error, "start_file::write"))
}

pub(crate) fn write_warmup_start_file(directory: &Path) -> Result<(), Warning> {
    std::fs::write(warmup_start_file(directory), b"warmup\n")
        .map_err(|error| warning_from_io(error, "warmup_start_file::write"))
}

pub(crate) fn create_ready_directory(directory: &Path) -> Result<(), Warning> {
    std::fs::create_dir_all(directory).map_err(|error| warning_from_io(error, "ready_dir::create"))
}

pub(crate) fn remove_ready_directory(directory: &Path) -> Result<(), Warning> {
    std::fs::remove_dir_all(directory).map_err(|error| warning_from_io(error, "ready_dir::remove"))
}

pub(crate) fn wait_paths_or_worker_exit(
    paths: &[PathBuf],
    workers: &mut Vec<Child>,
) -> Result<Option<Child>, Warning> {
    for _attempt in 0..READY_WAIT_ATTEMPTS {
        if paths.iter().all(|path| path.exists()) {
            return Ok(None);
        }
        if let Some(worker_index) = exited_worker_index(workers)? {
            return Ok(Some(workers.remove(worker_index)));
        }
        std::thread::sleep(Duration::from_millis(READY_WAIT_POLL_INTERVAL_MS));
    }
    Err(warning("wait_paths::timeout"))
}

fn named_ready_file(directory: &Path, prefix: &str, index: u8) -> PathBuf {
    directory.join(format!("{prefix}-{index}.ready"))
}

fn exited_worker_index(workers: &mut [Child]) -> Result<Option<usize>, Warning> {
    for (worker_index, worker) in workers.iter_mut().enumerate() {
        if worker
            .try_wait()
            .map_err(|error| warning_from_io(error, "wait_ready_files::try_wait"))?
            .is_some()
        {
            return Ok(Some(worker_index));
        }
    }
    Ok(None)
}

fn warning(operation: &'static str) -> Warning {
    Warning::new(WarningCode::InvalidConfig, COMPONENT_RBUS_BENCH, operation)
}

fn warning_from_io(error: std::io::Error, operation: &'static str) -> Warning {
    match error.raw_os_error() {
        Some(errno) => Warning::new(WarningCode::SyscallFailed, COMPONENT_RBUS_BENCH, operation)
            .with_os_errno(errno),
        None => Warning::new(WarningCode::SyscallFailed, COMPONENT_RBUS_BENCH, operation),
    }
}
