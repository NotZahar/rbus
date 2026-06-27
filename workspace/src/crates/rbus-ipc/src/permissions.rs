//! Permission helpers
use std::fs;
use std::path::Path;

use rbus_core::warning::{Warning, WarningCode};

/// Linux IPC metadata
pub fn ensure_private_directory(path: &Path) -> Result<(), Warning> {
    fs::create_dir_all(path).map_err(|error| {
        Warning::new(
            WarningCode::PermissionDenied,
            "rbus-ipc",
            "ensure_private_directory",
        )
        .with_os_errno(error.raw_os_error().unwrap_or(0))
    })
}
