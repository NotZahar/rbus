//! Unix socket helpers
use std::fs;
use std::io;
use std::os::fd::FromRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::path::PathBuf;

use rbus_core::warning::{Warning, WarningCode};

use crate::errno::warning_from_errno;
use crate::fd::OwnedFileDescriptor;

const COMPONENT_IPC: &str = "rbus-ipc";
const NUL_BYTE: u8 = 0;
const NO_REMOVED_SOCKET_COUNT: u64 = 0;
const REMOVED_SOCKET_COUNT: u64 = 1;
const SOCKET_DIRECTORY_MODE: u32 = 0o700;
const UNIX_SOCKET_LISTEN_BACKLOG: libc::c_int = 128;
const UNIX_SOCKET_PATH_TERMINATOR_LENGTH: usize = 1;
const UNIX_SOCKET_PROTOCOL_DEFAULT: libc::c_int = 0;

/// Returns the default daemon socket path for a domain
pub fn default_socket_path(domain: &str) -> PathBuf {
    runtime_directory_path()
        .join("rbus")
        .join(format!("{domain}.sock"))
}

/// Returns the per-user runtime directory used for sockets
pub fn runtime_directory_path() -> PathBuf {
    PathBuf::from(format!("/run/user/{}", effective_user_id()))
}

/// Binds a `SOCK_SEQPACKET` Unix listener at `socket_path`
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
    let file_descriptor = create_seqpacket_socket("bind_unix_listener::socket")?;
    let socket_address = UnixSocketAddress::new(socket_path, "bind_unix_listener::path")?;
    // SAFETY: file_descriptor is a valid AF_UNIX socket and socket_address points to a live sockaddr_un
    let bind_result = unsafe {
        libc::bind(
            file_descriptor.as_raw_file_descriptor(),
            socket_address.as_pointer(),
            socket_address.length(),
        )
    };
    if bind_result != 0 {
        return Err(warning_from_errno(
            WarningCode::SyscallFailed,
            COMPONENT_IPC,
            "bind_unix_listener::bind",
        ));
    }
    // SAFETY: file_descriptor is a bound AF_UNIX SOCK_SEQPACKET socket
    let listen_result = unsafe {
        libc::listen(
            file_descriptor.as_raw_file_descriptor(),
            UNIX_SOCKET_LISTEN_BACKLOG,
        )
    };
    if listen_result != 0 {
        return Err(warning_from_errno(
            WarningCode::SyscallFailed,
            COMPONENT_IPC,
            "bind_unix_listener::listen",
        ));
    }
    // SAFETY: ownership of the listening socket descriptor moves into UnixListener
    Ok(unsafe { UnixListener::from_raw_fd(file_descriptor.into_raw_file_descriptor()) })
}

/// Removes a stale socket pathname when no daemon is accepting on it
pub fn cleanup_stale_unix_socket(socket_path: &Path) -> Result<u64, Warning> {
    if !socket_path.exists() {
        return Ok(NO_REMOVED_SOCKET_COUNT);
    }
    match connect_unix_stream(socket_path) {
        Ok(_stream) => Ok(NO_REMOVED_SOCKET_COUNT),
        Err(warning) if is_stale_socket_warning(&warning) => {
            fs::remove_file(socket_path).map_err(|remove_error| {
                warning_from_io(remove_error, WarningCode::SyscallFailed, "remove_file")
            })?;
            Ok(REMOVED_SOCKET_COUNT)
        }
        Err(warning) => Err(warning),
    }
}

/// Connects to an existing daemon Unix socket
pub fn connect_unix_stream(socket_path: &Path) -> Result<UnixStream, Warning> {
    let file_descriptor = create_seqpacket_socket("connect_unix_stream::socket")?;
    let socket_address = UnixSocketAddress::new(socket_path, "connect_unix_stream::path")?;
    // SAFETY: file_descriptor is a valid AF_UNIX socket and socket_address points to a live sockaddr_un
    let connect_result = unsafe {
        libc::connect(
            file_descriptor.as_raw_file_descriptor(),
            socket_address.as_pointer(),
            socket_address.length(),
        )
    };
    if connect_result != 0 {
        return Err(warning_from_errno(
            WarningCode::DaemonUnavailable,
            COMPONENT_IPC,
            "connect_unix_stream::connect",
        ));
    }
    // SAFETY: ownership of the connected socket descriptor moves into UnixStream
    Ok(unsafe { UnixStream::from_raw_fd(file_descriptor.into_raw_file_descriptor()) })
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

fn is_stale_socket_warning(warning: &Warning) -> bool {
    matches!(
        warning.os_errno(),
        Some(libc::ECONNREFUSED | libc::ENOENT | libc::ENOTSOCK)
    )
}

fn effective_user_id() -> libc::uid_t {
    // SAFETY: `geteuid` has no pointer arguments and returns the effective user id
    unsafe { libc::geteuid() }
}

fn create_seqpacket_socket(operation: &'static str) -> Result<OwnedFileDescriptor, Warning> {
    // SAFETY: socket is called with constant AF_UNIX/SOCK_SEQPACKET parameters
    let raw_file_descriptor = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
            UNIX_SOCKET_PROTOCOL_DEFAULT,
        )
    };
    if raw_file_descriptor < 0 {
        return Err(warning_from_errno(
            WarningCode::SyscallFailed,
            COMPONENT_IPC,
            operation,
        ));
    }
    OwnedFileDescriptor::new(raw_file_descriptor)
}

struct UnixSocketAddress {
    address: libc::sockaddr_un,
    length: libc::socklen_t,
}

impl UnixSocketAddress {
    fn new(socket_path: &Path, operation: &'static str) -> Result<Self, Warning> {
        let path_bytes = socket_path.as_os_str().as_bytes();
        if path_bytes.is_empty()
            || path_bytes.contains(&NUL_BYTE)
            || path_bytes.len() + UNIX_SOCKET_PATH_TERMINATOR_LENGTH
                > core::mem::size_of::<libc::sockaddr_un>()
                    - core::mem::offset_of!(libc::sockaddr_un, sun_path)
        {
            return Err(Warning::new(
                WarningCode::InvalidConfig,
                COMPONENT_IPC,
                operation,
            ));
        }

        // SAFETY: zeroed sockaddr_un is initialized below before syscall use
        let mut address = unsafe { core::mem::zeroed::<libc::sockaddr_un>() };
        address.sun_family = libc::sa_family_t::try_from(libc::AF_UNIX).map_err(|_| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_IPC,
                "UnixSocketAddress::family",
            )
        })?;
        for (index, byte) in path_bytes.iter().copied().enumerate() {
            address.sun_path[index] = byte as libc::c_char;
        }
        let length = core::mem::offset_of!(libc::sockaddr_un, sun_path)
            .checked_add(path_bytes.len())
            .and_then(|value| value.checked_add(UNIX_SOCKET_PATH_TERMINATOR_LENGTH))
            .and_then(|value| libc::socklen_t::try_from(value).ok())
            .ok_or_else(|| {
                Warning::new(
                    WarningCode::ResourceLimit,
                    COMPONENT_IPC,
                    "UnixSocketAddress::length",
                )
            })?;
        Ok(Self { address, length })
    }

    fn as_pointer(&self) -> *const libc::sockaddr {
        core::ptr::from_ref(&self.address).cast::<libc::sockaddr>()
    }

    const fn length(&self) -> libc::socklen_t {
        self.length
    }
}
