//! Unix socket helpers
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::path::PathBuf;

use rbus_core::warning::{Warning, WarningCode};

const COMPONENT_IPC: &str = "rbus-ipc";
const NO_REMOVED_SOCKET_COUNT: u64 = 0;
const REMOVED_SOCKET_COUNT: u64 = 1;
const SOCKET_DIRECTORY_MODE: u32 = 0o700;

/// Linux IPC metadata
pub fn default_socket_path(domain: &str) -> PathBuf {
    runtime_directory_path()
        .join("rbus")
        .join(format!("{domain}.sock"))
}

/// Returns a value
pub fn runtime_directory_path() -> PathBuf {
    PathBuf::from(format!("/run/user/{}", effective_user_id()))
}

/// Linux IPC metadata
pub fn bind_unix_listener(socket_path: &Path) -> Result<UnixListener, Warning> {
    let parent = socket_path.parent().ok_or_else(|| {
        Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_IPC,
            "bind_unix_listener::parent",
        )
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| warning_from_io(error, WarningCode::SyscallFailed, "create_dir_all"))?;
    fs::set_permissions(parent, fs::Permissions::from_mode(SOCKET_DIRECTORY_MODE)).map_err(
        |error| warning_from_io(error, WarningCode::PermissionDenied, "set_permissions"),
    )?;
    let _removed_count = cleanup_stale_unix_socket(socket_path)?;
    UnixListener::bind(socket_path)
        .map_err(|error| warning_from_io(error, WarningCode::SyscallFailed, "UnixListener::bind"))
}

/// Daemon runtime metadata
pub fn cleanup_stale_unix_socket(socket_path: &Path) -> Result<u64, Warning> {
    if !socket_path.exists() {
        return Ok(NO_REMOVED_SOCKET_COUNT);
    }
    match UnixStream::connect(socket_path) {
        Ok(_stream) => Ok(NO_REMOVED_SOCKET_COUNT),
        Err(error) if is_stale_socket_error(&error) => {
            fs::remove_file(socket_path).map_err(|remove_error| {
                warning_from_io(remove_error, WarningCode::SyscallFailed, "remove_file")
            })?;
            Ok(REMOVED_SOCKET_COUNT)
        }
        Err(error) => Err(warning_from_io(
            error,
            WarningCode::SyscallFailed,
            "cleanup_stale_unix_socket::connect",
        )),
    }
}

/// Daemon runtime metadata
pub fn connect_unix_stream(socket_path: &Path) -> Result<UnixStream, Warning> {
    UnixStream::connect(socket_path).map_err(|error| {
        warning_from_io(error, WarningCode::DaemonUnavailable, "UnixStream::connect")
    })
}

fn warning_from_io(
    error: io::Error,
    warning_code: WarningCode,
    operation: &'static str,
) -> Warning {
    match error.raw_os_error() {
        Some(errno) => Warning::new(warning_code, COMPONENT_IPC, operation).with_os_errno(errno),
        None => Warning::new(warning_code, COMPONENT_IPC, operation),
    }
}

fn is_stale_socket_error(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(libc::ECONNREFUSED | libc::ENOENT | libc::ENOTSOCK)
    )
}

fn effective_user_id() -> libc::uid_t {
    // SAFETY: Required invariants are upheld
    unsafe { libc::geteuid() }
}
