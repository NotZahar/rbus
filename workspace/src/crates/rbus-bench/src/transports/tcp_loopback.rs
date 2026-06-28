use std::net::{SocketAddrV4, TcpListener};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};

use rbus_core::warning::{Warning, WarningCode};

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const SOCKET_OPTION_ENABLED: libc::c_int = 1;
const TCP_LISTEN_BACKLOG: libc::c_int = 128;

pub const TRANSPORT_NAME: &str = "tcp-loopback";

pub(crate) fn bind_reusable_listener(address: SocketAddrV4) -> Result<TcpListener, Warning> {
    // SAFETY: socket is called with Linux TCP/IPv4 constants and returns a new owned fd.
    let raw_fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0) };
    if raw_fd < 0 {
        return Err(warning_from_errno("tcp_socket"));
    }
    // SAFETY: raw_fd was just returned by socket and is owned by this function.
    let fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
    set_reuse_address(&fd)?;
    bind_socket_address(&fd, address)?;
    listen_socket(&fd)?;
    // SAFETY: into_raw_fd transfers ownership to TcpListener exactly once.
    Ok(unsafe { TcpListener::from_raw_fd(fd.into_raw_fd()) })
}

fn set_reuse_address(fd: &OwnedFd) -> Result<(), Warning> {
    let option = SOCKET_OPTION_ENABLED;
    // SAFETY: fd is valid and option points to an initialized c_int.
    let result = unsafe {
        libc::setsockopt(
            fd.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_REUSEADDR,
            core::ptr::addr_of!(option).cast(),
            core::mem::size_of_val(&option) as libc::socklen_t,
        )
    };
    if result != 0 {
        return Err(warning_from_errno("tcp_setsockopt"));
    }
    Ok(())
}

fn bind_socket_address(fd: &OwnedFd, address: SocketAddrV4) -> Result<(), Warning> {
    let socket_address = libc::sockaddr_in {
        sin_family: libc::AF_INET as libc::sa_family_t,
        sin_port: address.port().to_be(),
        sin_addr: libc::in_addr {
            s_addr: u32::from_ne_bytes(address.ip().octets()),
        },
        sin_zero: [0; 8],
    };
    // SAFETY: fd is valid and socket_address is a correctly initialized IPv4 sockaddr.
    let result = unsafe {
        libc::bind(
            fd.as_raw_fd(),
            core::ptr::addr_of!(socket_address).cast(),
            core::mem::size_of_val(&socket_address) as libc::socklen_t,
        )
    };
    if result != 0 {
        return Err(warning_from_errno("tcp_bind"));
    }
    Ok(())
}

fn listen_socket(fd: &OwnedFd) -> Result<(), Warning> {
    // SAFETY: fd is a bound TCP socket owned by this process.
    let result = unsafe { libc::listen(fd.as_raw_fd(), TCP_LISTEN_BACKLOG) };
    if result != 0 {
        return Err(warning_from_errno("tcp_listen"));
    }
    Ok(())
}

fn current_errno() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EINVAL)
}

fn warning_from_errno(operation: &'static str) -> Warning {
    Warning::new(WarningCode::SyscallFailed, COMPONENT_RBUS_BENCH, operation)
        .with_os_errno(current_errno())
}
