/// Subscriber runtime metadata
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitMode {
    /// Internal documentation
    BusyPoll,
    /// Blocking `recv`
    Recv,
    /// Blocking `recv_timeout`
    RecvTimeout,
}

/// Benchmark metadata
pub const WAIT_MODES: &[WaitMode] = &[WaitMode::BusyPoll, WaitMode::Recv, WaitMode::RecvTimeout];

impl WaitMode {
    /// Benchmark metadata
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "busy-poll" => Some(Self::BusyPoll),
            "recv" => Some(Self::Recv),
            "recv-timeout" => Some(Self::RecvTimeout),
            _ => None,
        }
    }

    /// Benchmark metadata
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BusyPoll => "busy-poll",
            Self::Recv => "recv",
            Self::RecvTimeout => "recv-timeout",
        }
    }

    /// Returns a value
    pub const fn uses_blocking_wait(self) -> bool {
        match self {
            Self::BusyPoll => false,
            Self::Recv | Self::RecvTimeout => true,
        }
    }
}
