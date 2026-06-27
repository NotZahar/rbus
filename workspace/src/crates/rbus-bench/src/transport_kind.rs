use crate::topology::ScenarioKind;
use crate::transports::{
    pipe, posix_mq, rbus_loaned, rbus_publish_bytes, tcp_loopback, unix_socket,
};

/// Benchmark metadata
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportKind {
    /// Zero-copy loaned path
    RbusLoaned,
    /// Convenience `publish_bytes` path
    RbusPublishBytes,
    /// Unix domain socket stream baseline
    UnixSocket,
    /// FIFO pipe baseline
    Pipe,
    /// TCP loopback stream baseline
    TcpLoopback,
    /// POSIX message queue baseline
    PosixMessageQueue,
}

/// Benchmark metadata
pub const SUITE_TRANSPORTS: &[TransportKind] = &[
    TransportKind::RbusLoaned,
    TransportKind::RbusPublishBytes,
    TransportKind::UnixSocket,
    TransportKind::Pipe,
    TransportKind::TcpLoopback,
    TransportKind::PosixMessageQueue,
];

impl TransportKind {
    /// Benchmark metadata
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            rbus_loaned::TRANSPORT_NAME => Some(Self::RbusLoaned),
            rbus_publish_bytes::TRANSPORT_NAME => Some(Self::RbusPublishBytes),
            unix_socket::TRANSPORT_NAME => Some(Self::UnixSocket),
            pipe::TRANSPORT_NAME => Some(Self::Pipe),
            tcp_loopback::TRANSPORT_NAME => Some(Self::TcpLoopback),
            posix_mq::TRANSPORT_NAME => Some(Self::PosixMessageQueue),
            _ => None,
        }
    }

    /// Benchmark metadata
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RbusLoaned => rbus_loaned::TRANSPORT_NAME,
            Self::RbusPublishBytes => rbus_publish_bytes::TRANSPORT_NAME,
            Self::UnixSocket => unix_socket::TRANSPORT_NAME,
            Self::Pipe => pipe::TRANSPORT_NAME,
            Self::TcpLoopback => tcp_loopback::TRANSPORT_NAME,
            Self::PosixMessageQueue => posix_mq::TRANSPORT_NAME,
        }
    }

    /// Benchmark metadata
    pub const fn is_rbus(self) -> bool {
        matches!(self, Self::RbusLoaned | Self::RbusPublishBytes)
    }

    /// Benchmark metadata
    pub const fn supports_scenario(self, scenario_kind: ScenarioKind) -> bool {
        self.is_rbus()
            || matches!(
                scenario_kind,
                ScenarioKind::OneWay | ScenarioKind::PingPong | ScenarioKind::SlowSubscriber
            )
    }
}
