use crate::topology::ScenarioKind;
use crate::transports::{
    rbus_loaned_fill, rbus_loaned_metadata_only, rbus_publish_bytes, unix_socket,
    unix_socket_end_to_end,
};

/// Transport implementation selected for a benchmark row
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportKind {
    /// Zero-copy loaned metadata path
    RbusLoanedMetadataOnly,
    /// Zero-copy loaned fill path
    RbusLoanedFill,
    /// Convenience `publish_bytes` path
    RbusPublishBytes,
    /// Unix domain socket stream baseline
    UnixSocket,
    /// Unix domain socket stream end-to-end baseline
    UnixSocketEndToEnd,
}

/// Transports covered by the full suite matrix
pub const SUITE_TRANSPORTS: &[TransportKind] = &[
    TransportKind::RbusLoanedMetadataOnly,
    TransportKind::RbusLoanedFill,
    TransportKind::RbusPublishBytes,
    TransportKind::UnixSocket,
    TransportKind::UnixSocketEndToEnd,
];

/// Transports covered by paced benchmark runs
pub const PACED_TRANSPORTS: &[TransportKind] = &[
    TransportKind::RbusLoanedMetadataOnly,
    TransportKind::RbusLoanedFill,
    TransportKind::RbusPublishBytes,
    TransportKind::UnixSocket,
];

impl TransportKind {
    /// Parses a transport CLI/report value
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            rbus_loaned_metadata_only::TRANSPORT_NAME => Some(Self::RbusLoanedMetadataOnly),
            rbus_loaned_fill::TRANSPORT_NAME => Some(Self::RbusLoanedFill),
            rbus_publish_bytes::TRANSPORT_NAME => Some(Self::RbusPublishBytes),
            unix_socket::TRANSPORT_NAME => Some(Self::UnixSocket),
            unix_socket_end_to_end::TRANSPORT_NAME => Some(Self::UnixSocketEndToEnd),
            _ => None,
        }
    }

    /// Returns the CLI/report string for this transport
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RbusLoanedMetadataOnly => rbus_loaned_metadata_only::TRANSPORT_NAME,
            Self::RbusLoanedFill => rbus_loaned_fill::TRANSPORT_NAME,
            Self::RbusPublishBytes => rbus_publish_bytes::TRANSPORT_NAME,
            Self::UnixSocket => unix_socket::TRANSPORT_NAME,
            Self::UnixSocketEndToEnd => unix_socket_end_to_end::TRANSPORT_NAME,
        }
    }

    /// Returns whether this transport uses rbus shared-memory data plane
    pub const fn is_rbus(self) -> bool {
        matches!(
            self,
            Self::RbusLoanedMetadataOnly | Self::RbusLoanedFill | Self::RbusPublishBytes
        )
    }

    /// Returns whether this transport supports a scenario kind
    pub const fn supports_scenario(self, scenario_kind: ScenarioKind) -> bool {
        match scenario_kind {
            ScenarioKind::OneWay | ScenarioKind::SlowSubscriber => true,
        }
    }

    pub const fn fills_payload_in_measured_window(self) -> bool {
        matches!(self, Self::RbusLoanedFill | Self::UnixSocketEndToEnd)
    }
}
