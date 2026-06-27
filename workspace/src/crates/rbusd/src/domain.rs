use std::path::PathBuf;

use rbus_ipc::unix_socket::default_socket_path;

/// Linux IPC metadata
pub fn domain_socket_path(domain: &str) -> PathBuf {
    default_socket_path(domain)
}
