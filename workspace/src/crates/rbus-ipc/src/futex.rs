//! Internal documentation

use core::sync::atomic::AtomicU32;
use core::time::Duration;

use rbus_core::result::RbusResult;
use rbus_core::warning::{Warning, WarningCode};

use crate::errno::warning_from_errno;

const COMPONENT_IPC: &str = "rbus-ipc";
const NANOSECONDS_PER_SECOND: u64 = 1_000_000_000;

/// Linux IPC metadata
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FutexWaitStatus {
    /// Internal documentation
    Woken,
    /// Internal documentation
    ValueChanged,
    /// Internal documentation
    TimedOut,
}

/// Linux IPC metadata
pub fn futex_wait_private(futex_word: &AtomicU32, expected: u32) -> RbusResult<()> {
    let futex_pointer = atomic_u32_as_futex_pointer(futex_word);

    // SAFETY: Required invariants are upheld
    // Invariants:
    // - Required invariant holds
    // - Required invariant holds
    // Covered by tests:
    // - futex_wait_private_returns_warning_when_value_mismatches
    // - futex_wake_private_returns_zero_without_waiters
    let result = unsafe {
        libc::syscall(
            libc::SYS_futex,
            futex_pointer,
            libc::FUTEX_WAIT | libc::FUTEX_PRIVATE_FLAG,
            expected,
            core::ptr::null::<libc::timespec>(),
        )
    };
    if result != 0 {
        return Err(warning_from_errno(
            WarningCode::SyscallFailed,
            COMPONENT_IPC,
            "futex_wait_private",
        ));
    }
    Ok(())
}

/// Linux IPC metadata
pub fn futex_wake_private(futex_word: &AtomicU32, wake_count: u32) -> RbusResult<u32> {
    let wake_count = libc::c_int::try_from(wake_count).map_err(|_| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_IPC,
            "futex_wake_private::wake_count",
        )
    })?;
    let futex_pointer = atomic_u32_as_futex_pointer(futex_word);

    // SAFETY: Required invariants are upheld
    // Invariants:
    // - Required invariant holds
    // - Required invariant holds
    // Covered by tests:
    // - futex_wake_private_returns_zero_without_waiters
    let result = unsafe {
        libc::syscall(
            libc::SYS_futex,
            futex_pointer,
            libc::FUTEX_WAKE | libc::FUTEX_PRIVATE_FLAG,
            wake_count,
        )
    };
    if result < 0 {
        return Err(warning_from_errno(
            WarningCode::SyscallFailed,
            COMPONENT_IPC,
            "futex_wake_private",
        ));
    }
    u32::try_from(result).map_err(|_| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_IPC,
            "futex_wake_private::result",
        )
    })
}

/// Linux IPC metadata
pub fn futex_wait_shared(
    futex_word: &AtomicU32,
    expected: u32,
    timeout: Option<Duration>,
) -> RbusResult<FutexWaitStatus> {
    let futex_pointer = atomic_u32_as_futex_pointer(futex_word);
    let timeout_timespec = match timeout {
        Some(timeout) => Some(relative_timespec(timeout)?),
        None => None,
    };
    let timeout_pointer = timeout_timespec
        .as_ref()
        .map_or(core::ptr::null(), core::ptr::from_ref);

    // SAFETY: Required invariants are upheld
    // Internal note
    // Covered by the named test
    let result = unsafe {
        libc::syscall(
            libc::SYS_futex,
            futex_pointer,
            libc::FUTEX_WAIT,
            expected,
            timeout_pointer,
        )
    };
    futex_wait_status(result, "futex_wait_shared")
}

/// Linux IPC metadata
pub fn futex_wake_shared(futex_word: &AtomicU32, wake_count: u32) -> RbusResult<u32> {
    let wake_count = libc::c_int::try_from(wake_count).map_err(|_| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_IPC,
            "futex_wake_shared::wake_count",
        )
    })?;
    let futex_pointer = atomic_u32_as_futex_pointer(futex_word);

    // SAFETY: Required invariants are upheld
    // Covered by the named test
    let result =
        unsafe { libc::syscall(libc::SYS_futex, futex_pointer, libc::FUTEX_WAKE, wake_count) };
    if result < 0 {
        return Err(warning_from_errno(
            WarningCode::SyscallFailed,
            COMPONENT_IPC,
            "futex_wake_shared",
        ));
    }
    u32::try_from(result).map_err(|_| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_IPC,
            "futex_wake_shared::result",
        )
    })
}

fn futex_wait_status(result: libc::c_long, operation: &'static str) -> RbusResult<FutexWaitStatus> {
    if result >= 0 {
        return Ok(FutexWaitStatus::Woken);
    }

    match crate::errno::current_errno() {
        libc::EAGAIN => Ok(FutexWaitStatus::ValueChanged),
        libc::ETIMEDOUT => Ok(FutexWaitStatus::TimedOut),
        _ => Err(warning_from_errno(
            WarningCode::SyscallFailed,
            COMPONENT_IPC,
            operation,
        )),
    }
}

fn relative_timespec(duration: Duration) -> RbusResult<libc::timespec> {
    timespec_from_nanoseconds(u64::try_from(duration.as_nanos()).map_err(|_| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_IPC,
            "relative_timespec::nanoseconds",
        )
    })?)
}

fn timespec_from_nanoseconds(nanoseconds: u64) -> RbusResult<libc::timespec> {
    let seconds = nanoseconds / NANOSECONDS_PER_SECOND;
    let subsecond_nanoseconds = nanoseconds % NANOSECONDS_PER_SECOND;
    Ok(libc::timespec {
        tv_sec: libc::time_t::try_from(seconds).map_err(|_| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_IPC,
                "timespec_from_nanoseconds::seconds",
            )
        })?,
        tv_nsec: libc::c_long::try_from(subsecond_nanoseconds).map_err(|_| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_IPC,
                "timespec_from_nanoseconds::nanoseconds",
            )
        })?,
    })
}

fn atomic_u32_as_futex_pointer(futex_word: &AtomicU32) -> *mut libc::c_int {
    core::ptr::from_ref(futex_word)
        .cast::<libc::c_int>()
        .cast_mut()
}
