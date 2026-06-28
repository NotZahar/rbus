use crate::args::SuiteOptions;
use crate::payload::PAYLOAD_SIZES;
use crate::topology::{ScenarioKind, TOPOLOGIES, Topology};
use crate::wait_mode::{WAIT_MODES, WaitMode};
use crate::worker::warmup_messages_per_publisher;

const DEPTH_MINIMUM: usize = 8;

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

pub(crate) fn selected_wait_modes(options: &SuiteOptions) -> &'static [WaitMode] {
    if options.matrix_smoke {
        &WAIT_MODES[..1]
    } else {
        WAIT_MODES
    }
}

pub(crate) fn measured_messages(
    scenario_kind: ScenarioKind,
    publisher_count: u8,
    subscriber_count: u8,
    messages_per_publisher: u32,
) -> u64 {
    match scenario_kind {
        ScenarioKind::PingPong | ScenarioKind::CrashSubscriber | ScenarioKind::CrashPublisher => {
            u64::from(messages_per_publisher)
        }
        ScenarioKind::OneWay | ScenarioKind::SlowSubscriber => {
            u64::from(publisher_count)
                * u64::from(subscriber_count)
                * u64::from(messages_per_publisher)
        }
    }
}

pub(crate) fn depth_for(messages_per_publisher: u32) -> usize {
    let total_messages = messages_per_publisher
        .saturating_add(warmup_messages_per_publisher())
        .saturating_add(1);
    DEPTH_MINIMUM.max(total_messages.next_power_of_two() as usize)
}
