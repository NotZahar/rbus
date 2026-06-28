/// Benchmark metadata
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenarioKind {
    /// One-way publish/receive
    OneWay,
    /// Warning model metadata
    SlowSubscriber,
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
pub const TOPOLOGIES: &[Topology] = &[Topology {
    name: "four-publishers-eight-subscribers",
    publisher_count: 4,
    subscriber_count: 8,
    kind: ScenarioKind::OneWay,
}];
