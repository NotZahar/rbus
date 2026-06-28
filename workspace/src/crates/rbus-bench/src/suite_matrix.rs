use crate::args::SuiteOptions;
use crate::payload::PAYLOAD_SIZES;
use crate::topology::{ScenarioKind, TOPOLOGIES, Topology};
use crate::wait_mode::{WAIT_MODES, WaitMode};

const DEPTH_MINIMUM: usize = 8;
const MAX_PAYLOAD_BYTES_PER_STREAM: usize = 512 * 1024 * 1024;
const POWER_OF_TWO_FACTOR: usize = 2;

const _: () = assert!(DEPTH_MINIMUM.is_power_of_two());

pub(crate) fn selected_topologies(options: &SuiteOptions) -> &'static [Topology] {
    if options.matrix_smoke {
        &TOPOLOGIES[..1]
    } else {
        TOPOLOGIES
    }
}

pub(crate) fn selected_payload_sizes(options: &SuiteOptions) -> &'static [usize] {
    if options.matrix_smoke {
        &PAYLOAD_SIZES[..1]
    } else {
        PAYLOAD_SIZES
    }
}

pub(crate) fn selected_wait_modes(_options: &SuiteOptions) -> &'static [WaitMode] {
    &WAIT_MODES[..1]
}

pub(crate) fn messages_per_publisher_for_payload(
    options: &SuiteOptions,
    payload_size: usize,
) -> u32 {
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
