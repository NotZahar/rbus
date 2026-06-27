use crate::transports::{rbus_loaned, rbus_publish_bytes};

/// Benchmark metadata
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportKind {
    /// Zero-copy loaned path
    RbusLoaned,
    /// Convenience `publish_bytes` path
    RbusPublishBytes,
}

/// Benchmark metadata
pub const RBUS_TRANSPORTS: &[TransportKind] =
    &[TransportKind::RbusLoaned, TransportKind::RbusPublishBytes];

impl TransportKind {
    /// Benchmark metadata
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            rbus_loaned::TRANSPORT_NAME => Some(Self::RbusLoaned),
            rbus_publish_bytes::TRANSPORT_NAME => Some(Self::RbusPublishBytes),
            _ => None,
        }
    }

    /// Benchmark metadata
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RbusLoaned => rbus_loaned::TRANSPORT_NAME,
            Self::RbusPublishBytes => rbus_publish_bytes::TRANSPORT_NAME,
        }
    }
}
