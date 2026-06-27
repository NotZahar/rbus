/// Benchmark metadata
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenarioKind {
    /// One-way publish/receive
    OneWay,
    /// Ping-pong round-trip
    PingPong,
    /// Warning model metadata
    SlowSubscriber,
    /// Subscriber runtime metadata
    CrashSubscriber,
    /// Publisher runtime metadata
    CrashPublisher,
}

/// Benchmark topology
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Topology {
    /// Stable scenario name
    pub name: &'static str,
    /// Publisher count
    pub publisher_count: u8,
    /// Subscriber count
    pub subscriber_count: u8,
    /// Scenario kind
    pub kind: ScenarioKind,
}

/// Benchmark metadata
pub const TOPOLOGIES: &[Topology] = &[
    Topology {
        name: "one-publisher-one-subscriber",
        publisher_count: 1,
        subscriber_count: 1,
        kind: ScenarioKind::OneWay,
    },
    Topology {
        name: "one-publisher-two-subscribers",
        publisher_count: 1,
        subscriber_count: 2,
        kind: ScenarioKind::OneWay,
    },
    Topology {
        name: "one-publisher-four-subscribers",
        publisher_count: 1,
        subscriber_count: 4,
        kind: ScenarioKind::OneWay,
    },
    Topology {
        name: "two-publishers-one-subscriber",
        publisher_count: 2,
        subscriber_count: 1,
        kind: ScenarioKind::OneWay,
    },
    Topology {
        name: "four-publishers-one-subscriber",
        publisher_count: 4,
        subscriber_count: 1,
        kind: ScenarioKind::OneWay,
    },
    Topology {
        name: "four-publishers-four-subscribers",
        publisher_count: 4,
        subscriber_count: 4,
        kind: ScenarioKind::OneWay,
    },
    Topology {
        name: "ping-pong-round-trip",
        publisher_count: 1,
        subscriber_count: 1,
        kind: ScenarioKind::PingPong,
    },
    Topology {
        name: "slow-subscriber-gap-detection",
        publisher_count: 1,
        subscriber_count: 1,
        kind: ScenarioKind::SlowSubscriber,
    },
    Topology {
        name: "crash-subscriber-message-ref",
        publisher_count: 1,
        subscriber_count: 1,
        kind: ScenarioKind::CrashSubscriber,
    },
    Topology {
        name: "crash-publisher-stream-lifetime",
        publisher_count: 1,
        subscriber_count: 1,
        kind: ScenarioKind::CrashPublisher,
    },
];
