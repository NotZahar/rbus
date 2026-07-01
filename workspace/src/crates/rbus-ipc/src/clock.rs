//! Monotonic clock helpers

use rbus_core::warning::{Warning, WarningCode};

use crate::errno::warning_from_errno;

const COMPONENT_IPC: &str = "rbus-ipc";

/// Returns the current monotonic timestamp in nanoseconds
pub fn monotonic_timestamp_ns() -> Result<u64, Warning> {
    let mut timespec = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `timespec` points to a valid stack allocation that `clock_gettime` may fill
    let result =
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, core::ptr::addr_of_mut!(timespec)) };
    if result != 0 {
        return Err(warning_from_errno(
            WarningCode::SyscallFailed,
            COMPONENT_IPC,
            "monotonic_timestamp_ns",
        ));
    }
    let seconds = u64::try_from(timespec.tv_sec).map_err(|_| {
        Warning::new(
            WarningCode::InternalInvariantWarning,
            COMPONENT_IPC,
            "monotonic_timestamp_ns::seconds",
        )
    })?;
    let nanoseconds = u64::try_from(timespec.tv_nsec).map_err(|_| {
        Warning::new(
            WarningCode::InternalInvariantWarning,
            COMPONENT_IPC,
            "monotonic_timestamp_ns::nanoseconds",
        )
    })?;
    Ok(seconds
        .saturating_mul(1_000_000_000)
        .saturating_add(nanoseconds))
}
