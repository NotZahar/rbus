pub mod baseline_ipc;
pub(crate) mod baseline_paths;
pub mod rbus_loaned_fill;
pub mod rbus_loaned_metadata_only;
pub mod rbus_publish_bytes;
pub mod unix_socket;
pub mod unix_socket_end_to_end;

/// Transport names accepted by CLI filters and emitted in reports
pub const TRANSPORT_NAMES: &[&str] = &[
    rbus_loaned_metadata_only::TRANSPORT_NAME,
    rbus_loaned_fill::TRANSPORT_NAME,
    rbus_publish_bytes::TRANSPORT_NAME,
    unix_socket::TRANSPORT_NAME,
    unix_socket_end_to_end::TRANSPORT_NAME,
];
