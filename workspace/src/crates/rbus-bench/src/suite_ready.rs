use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::Duration;

use rbus_core::warning::{Warning, WarningCode};

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const READY_WAIT_ATTEMPTS: usize = 300_000;
const READY_WAIT_POLL_INTERVAL_MS: u64 = 1;

pub(crate) fn ready_directory(run_index: usize) -> PathBuf {
    std::env::temp_dir().join(format!("rbus-bench-{}-{run_index}", std::process::id()))
}

pub(crate) fn ready_file(directory: &Path, index: u8) -> PathBuf {
    directory.join(format!("subscriber-{index}.ready"))
}

pub(crate) fn create_ready_directory(directory: &Path) -> Result<(), Warning> {
    std::fs::create_dir_all(directory).map_err(|error| warning_from_io(error, "ready_dir::create"))
}

pub(crate) fn remove_ready_directory(directory: &Path) -> Result<(), Warning> {
    std::fs::remove_dir_all(directory).map_err(|error| warning_from_io(error, "ready_dir::remove"))
}

pub(crate) fn wait_ready_files(directory: &Path, count: u8) -> Result<(), Warning> {
    for _attempt in 0..READY_WAIT_ATTEMPTS {
        if ready_file_count(directory, count) == count {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(READY_WAIT_POLL_INTERVAL_MS));
    }
    Err(warning("wait_ready_files::timeout"))
}

pub(crate) fn wait_ready_files_or_worker_exit(
    directory: &Path,
    count: u8,
    workers: &mut Vec<Child>,
) -> Result<Option<Child>, Warning> {
    for _attempt in 0..READY_WAIT_ATTEMPTS {
        if ready_file_count(directory, count) == count {
            return Ok(None);
        }
        if let Some(worker_index) = exited_worker_index(workers)? {
            return Ok(Some(workers.remove(worker_index)));
        }
        std::thread::sleep(Duration::from_millis(READY_WAIT_POLL_INTERVAL_MS));
    }
    Err(warning("wait_ready_files::timeout"))
}

fn ready_file_count(directory: &Path, count: u8) -> u8 {
    let mut ready_count = 0_u8;
    for index in 0..count {
        if ready_file(directory, index).exists() {
            ready_count = ready_count.saturating_add(1);
        }
    }
    ready_count
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
