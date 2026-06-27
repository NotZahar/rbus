pub mod baseline_ipc;
pub(crate) mod baseline_paths;
pub mod pipe;
pub mod posix_mq;
pub(crate) mod posix_queue;
pub mod rbus_loaned;
pub mod rbus_publish_bytes;
pub mod tcp_loopback;
pub mod unix_socket;

/// Benchmark metadata
pub const TRANSPORT_NAMES: &[&str] = &[
    rbus_loaned::TRANSPORT_NAME,
    rbus_publish_bytes::TRANSPORT_NAME,
    unix_socket::TRANSPORT_NAME,
    pipe::TRANSPORT_NAME,
    tcp_loopback::TRANSPORT_NAME,
    posix_mq::TRANSPORT_NAME,
];
