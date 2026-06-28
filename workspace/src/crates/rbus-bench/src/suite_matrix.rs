use crate::args::SuiteOptions;
use crate::load_mode::LoadMode;
use crate::payload::PAYLOAD_SIZES;
use crate::topology::{PACED_TOPOLOGIES, ScenarioKind, TOPOLOGIES, Topology};
use crate::transport_kind::{PACED_TRANSPORTS, SUITE_TRANSPORTS, TransportKind};
use crate::wait_mode::{WAIT_MODES, WaitMode};

const DEPTH_MINIMUM: usize = 8;
const MAX_PAYLOAD_BYTES_PER_STREAM: usize = 512 * 1024 * 1024;
const POWER_OF_TWO_FACTOR: usize = 2;
const PACED_PAYLOAD_SIZES: &[usize] = &[64, 4 * 1024, 64 * 1024, 1024 * 1024, 8 * 1024 * 1024];

const _: () = assert!(DEPTH_MINIMUM.is_power_of_two());

pub(crate) fn selected_topologies(options: &SuiteOptions) -> Vec<Topology> {
    let source = if options.load_mode == LoadMode::Paced {
        PACED_TOPOLOGIES
    } else {
        TOPOLOGIES
    };
    let limit = if options.matrix_smoke {
        1
    } else {
        source.len()
    };
    source
        .iter()
        .copied()
        .filter(|topology| match options.scenario_filter.as_deref() {
            Some(scenario) => scenario == topology.name,
            None => true,
        })
        .take(limit)
        .collect()
}

pub(crate) fn selected_payload_sizes(options: &SuiteOptions) -> Vec<usize> {
    let source = if options.load_mode == LoadMode::Paced {
        PACED_PAYLOAD_SIZES
    } else {
        PAYLOAD_SIZES
    };
    let limit = if options.matrix_smoke {
        1
    } else {
        source.len()
    };
    source
        .iter()
        .copied()
        .filter(|payload_size| match options.payload_size_filter {
            Some(filter) => filter == *payload_size,
            None => true,
        })
        .take(limit)
        .collect()
}

pub(crate) fn selected_wait_modes(options: &SuiteOptions) -> Vec<WaitMode> {
    match options.wait_mode_filter {
        Some(filter) => WAIT_MODES
            .iter()
            .copied()
            .filter(|wait_mode| filter == *wait_mode)
            .collect(),
        None => WAIT_MODES.iter().copied().take(1).collect(),
    }
}

pub(crate) fn selected_transports(options: &SuiteOptions) -> Vec<TransportKind> {
    let source = if options.load_mode == LoadMode::Paced {
        PACED_TRANSPORTS
    } else {
        SUITE_TRANSPORTS
    };
    source
        .iter()
        .copied()
        .filter(|transport| match options.transport_filter {
            Some(filter) => filter == *transport,
            None => true,
        })
        .collect()
}

pub(crate) fn messages_per_publisher_for_payload(
    options: &SuiteOptions,
    payload_size: usize,
) -> u32 {
    if options.load_mode == LoadMode::Paced {
        return options.messages_per_publisher;
    }
    let capped_messages = byte_limited_depth(payload_size).saturating_sub(1) as u32;
    options.messages_per_publisher.min(capped_messages)
}

pub(crate) fn measured_messages(
    scenario_kind: ScenarioKind,
    publisher_count: u8,
    subscriber_count: u8,
    messages_per_publisher: u32,
) -> u64 {
    match scenario_kind {
        ScenarioKind::OneWay | ScenarioKind::SlowSubscriber => {
            u64::from(publisher_count)
                * u64::from(subscriber_count)
                * u64::from(messages_per_publisher)
        }
    }
}

pub(crate) fn depth_for(payload_size: usize, messages_per_publisher: u32) -> usize {
    let requested_depth = (messages_per_publisher as usize)
        .saturating_add(1)
        .next_power_of_two();
    DEPTH_MINIMUM.max(requested_depth.min(byte_limited_depth(payload_size)))
}

fn byte_limited_depth(payload_size: usize) -> usize {
    floor_power_of_two((MAX_PAYLOAD_BYTES_PER_STREAM / payload_size).max(DEPTH_MINIMUM))
}

fn floor_power_of_two(value: usize) -> usize {
    let mut power = 1_usize;
    while power <= value / POWER_OF_TWO_FACTOR {
        power *= POWER_OF_TWO_FACTOR;
    }
    power
}
