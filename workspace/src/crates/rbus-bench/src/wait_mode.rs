/// Subscriber wait strategy used by benchmark workers
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitMode {
    /// Poll `try_recv` without blocking
    BusyPoll,
    /// Blocking `recv`
    Recv,
    /// Blocking `recv_timeout`
    RecvTimeout,
}

/// Wait modes covered by the default benchmark matrix
pub const WAIT_MODES: &[WaitMode] = &[WaitMode::BusyPoll, WaitMode::Recv, WaitMode::RecvTimeout];

impl WaitMode {
    /// Parses a wait mode CLI value
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "busy-poll" => Some(Self::BusyPoll),
            "recv" => Some(Self::Recv),
            "recv-timeout" => Some(Self::RecvTimeout),
            _ => None,
        }
    }

    /// Returns the CLI/report string for this wait mode
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BusyPoll => "busy-poll",
            Self::Recv => "recv",
            Self::RecvTimeout => "recv-timeout",
        }
    }
}
