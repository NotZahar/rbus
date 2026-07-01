/// Scenario shape used by the benchmark runner
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenarioKind {
    /// One-way publish/receive
    OneWay,
    /// Scenario that intentionally creates subscriber lag
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

const FOUR_PUBLISHERS_EIGHT_SUBSCRIBERS: Topology = Topology {
    name: "four-publishers-eight-subscribers",
    publisher_count: 4,
    subscriber_count: 8,
    kind: ScenarioKind::OneWay,
};

const ONE_PUBLISHER_ONE_SUBSCRIBER: Topology = Topology {
    name: "one-publisher-one-subscriber",
    publisher_count: 1,
    subscriber_count: 1,
    kind: ScenarioKind::OneWay,
};

const ONE_PUBLISHER_FOUR_SUBSCRIBERS: Topology = Topology {
    name: "one-publisher-four-subscribers",
    publisher_count: 1,
    subscriber_count: 4,
    kind: ScenarioKind::OneWay,
};

/// Default saturated benchmark topologies
pub const TOPOLOGIES: &[Topology] = &[FOUR_PUBLISHERS_EIGHT_SUBSCRIBERS];

/// Topologies used by paced benchmark runs
pub const PACED_TOPOLOGIES: &[Topology] = &[
    ONE_PUBLISHER_ONE_SUBSCRIBER,
    ONE_PUBLISHER_FOUR_SUBSCRIBERS,
    FOUR_PUBLISHERS_EIGHT_SUBSCRIBERS,
];

pub fn topology_by_name(name: &str) -> Option<Topology> {
    for topology in PACED_TOPOLOGIES {
        if topology.name == name {
            return Some(*topology);
        }
    }
    None
}
