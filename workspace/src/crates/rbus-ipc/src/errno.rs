//! Conversion from OS errno values to structured warnings

use rbus_core::warning::{Warning, WarningCode};

/// Returns the current thread's last OS errno, or zero if unavailable
pub fn current_errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// Creates a warning and attaches the current OS errno
pub fn warning_from_errno(
    warning_code: WarningCode,
    component: &'static str,
    operation: &'static str,
) -> Warning {
    Warning::new(warning_code, component, operation).with_os_errno(current_errno())
}
