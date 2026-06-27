use std::fs;

use rbus_core::warning::{Warning, WarningCode};

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const NANOS_PER_SECOND: u64 = 1_000_000_000;
const STATUS_VOLUNTARY_CONTEXT_SWITCHES: &str = "voluntary_ctxt_switches:";
const STATUS_NONVOLUNTARY_CONTEXT_SWITCHES: &str = "nonvoluntary_ctxt_switches:";
const SMAPS_PSS: &str = "Pss:";
const SMAPS_RSS: &str = "Rss:";
const BYTES_PER_KIB: u64 = 1024;

/// Internal documentation
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProcessSystemSnapshot {
    /// Internal documentation
    pub cpu_time_ns: u64,
    /// Internal documentation
    pub context_switches: u64,
    /// Internal documentation
    pub rss_bytes: u64,
    /// Internal documentation
    pub pss_bytes: u64,
}

impl ProcessSystemSnapshot {
    /// Internal documentation
    pub fn capture() -> Result<Self, Warning> {
        Ok(Self {
            cpu_time_ns: process_cpu_time_ns()?,
            context_switches: context_switches()?,
            rss_bytes: memory_value_bytes(SMAPS_RSS)?,
            pss_bytes: memory_value_bytes(SMAPS_PSS)?,
        })
    }

    /// Returns a value
    pub fn delta(self, start: Self) -> Self {
        Self {
            cpu_time_ns: self.cpu_time_ns.saturating_sub(start.cpu_time_ns),
            context_switches: self.context_switches.saturating_sub(start.context_switches),
            rss_bytes: self.rss_bytes,
            pss_bytes: self.pss_bytes,
        }
    }
}

/// Returns a value
pub fn monotonic_time_ns() -> Result<u64, Warning> {
    clock_time_ns(libc::CLOCK_MONOTONIC, "monotonic_time_ns")
}

fn process_cpu_time_ns() -> Result<u64, Warning> {
    clock_time_ns(libc::CLOCK_PROCESS_CPUTIME_ID, "process_cpu_time_ns")
}

fn clock_time_ns(clock_id: libc::clockid_t, operation: &'static str) -> Result<u64, Warning> {
    let mut timespec = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: Required invariants are upheld
    let result = unsafe { libc::clock_gettime(clock_id, &mut timespec) };
    if result != 0 {
        return Err(warning_from_errno(operation));
    }
    let seconds = u64::try_from(timespec.tv_sec).map_err(|_| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_RBUS_BENCH,
            "clock_time_ns::seconds",
        )
    })?;
    let nanoseconds = u64::try_from(timespec.tv_nsec).map_err(|_| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_RBUS_BENCH,
            "clock_time_ns::nanoseconds",
        )
    })?;
    seconds
        .checked_mul(NANOS_PER_SECOND)
        .and_then(|value| value.checked_add(nanoseconds))
        .ok_or_else(|| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_RBUS_BENCH,
                "clock_time_ns::overflow",
            )
        })
}

fn context_switches() -> Result<u64, Warning> {
    let status = fs::read_to_string("/proc/self/status").map_err(|error| {
        warning_from_io(error, WarningCode::SyscallFailed, "context_switches::read")
    })?;
    let voluntary = proc_line_value(&status, STATUS_VOLUNTARY_CONTEXT_SWITCHES)?;
    let nonvoluntary = proc_line_value(&status, STATUS_NONVOLUNTARY_CONTEXT_SWITCHES)?;
    voluntary.checked_add(nonvoluntary).ok_or_else(|| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_RBUS_BENCH,
            "context_switches::overflow",
        )
    })
}

fn memory_value_bytes(key: &'static str) -> Result<u64, Warning> {
    let smaps = fs::read_to_string("/proc/self/smaps_rollup").map_err(|error| {
        warning_from_io(
            error,
            WarningCode::SyscallFailed,
            "memory_value_bytes::read",
        )
    })?;
    proc_line_value(&smaps, key)?
        .checked_mul(BYTES_PER_KIB)
        .ok_or_else(|| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_RBUS_BENCH,
                "memory_value_bytes::overflow",
            )
        })
}

fn proc_line_value(contents: &str, key: &'static str) -> Result<u64, Warning> {
    let line = contents
        .lines()
        .find(|line| line.starts_with(key))
        .ok_or_else(|| {
            Warning::new(
                WarningCode::InvalidConfig,
                COMPONENT_RBUS_BENCH,
                "proc_line_value::missing",
            )
        })?;
    let value = line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| {
            Warning::new(
                WarningCode::InvalidConfig,
                COMPONENT_RBUS_BENCH,
                "proc_line_value::field",
            )
        })?
        .parse::<u64>()
        .map_err(|_| {
            Warning::new(
                WarningCode::InvalidConfig,
                COMPONENT_RBUS_BENCH,
                "proc_line_value::parse",
            )
        })?;
    Ok(value)
}

fn warning_from_errno(operation: &'static str) -> Warning {
    let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
    Warning::new(WarningCode::SyscallFailed, COMPONENT_RBUS_BENCH, operation).with_os_errno(errno)
}

fn warning_from_io(
    error: std::io::Error,
    warning_code: WarningCode,
    operation: &'static str,
) -> Warning {
    match error.raw_os_error() {
        Some(errno) => {
            Warning::new(warning_code, COMPONENT_RBUS_BENCH, operation).with_os_errno(errno)
        }
        None => Warning::new(warning_code, COMPONENT_RBUS_BENCH, operation),
    }
}
