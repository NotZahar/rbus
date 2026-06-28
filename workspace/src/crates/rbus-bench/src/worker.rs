use std::time::Duration;

use rbus::{Bus, Publisher, Subscriber, TopicConfig};
use rbus_core::warning::{Warning, WarningCode};

use crate::allocation;
use crate::load_mode::LoadMode;
use crate::pace::{PaceSchedule, spin_tail_ns_from_us, wait_until};
use crate::system_metrics::{ProcessSystemSnapshot, monotonic_time_ns};
use crate::timestamp_payload::{PayloadTimestamps, decode_timestamps, encode_timestamps};
use crate::transport_kind::TransportKind;
use crate::wait_mode::WaitMode;
pub use crate::worker_args::{WorkerArguments, WorkerReport, WorkerRole};
use crate::worker_common::{
    invalid_argument, print_one_way_latencies, print_prefixed_samples, print_report,
    usize_from_u64, validate_worker_arguments, wait_start_file, wait_warmup_start_file,
    write_attach_ready_file, write_ready_file,
};

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const FILL_BYTE: u8 = 0xA5;
const PACING_PREWARM_SLEEP_NS: u64 = 1;
const SLOW_SUBSCRIBER_DELAY_MICROS: u64 = 100;
const RECV_TIMEOUT_MS: u64 = 5_000;
const PACED_DRAIN_TIMEOUT_MS: u64 = 5_000;
const UNSET_TIMESTAMP_NS: u64 = 0;

pub const TRANSPORT_LATENCY_PREFIX: &str = "RBUS_BENCH_TRANSPORT_LATENCY_NS ";
pub const END_TO_END_LATENCY_PREFIX: &str = "RBUS_BENCH_END_TO_END_LATENCY_NS ";
pub const ROUND_TRIP_PREFIX: &str = "RBUS_BENCH_ROUND_TRIP_NS ";
pub const PUBLISH_LATENESS_PREFIX: &str = "RBUS_BENCH_PUBLISH_LATENESS_NS ";
pub const INTER_PUBLISH_GAP_PREFIX: &str = "RBUS_BENCH_INTER_PUBLISH_GAP_NS ";
pub const INTER_RECEIVE_GAP_PREFIX: &str = "RBUS_BENCH_INTER_RECEIVE_GAP_NS ";
pub const BACKLOG_PREFIX: &str = "RBUS_BENCH_BACKLOG_MESSAGES ";
pub const RESULT_PREFIX: &str = "RBUS_BENCH_WORKER_RESULT ";

/// Benchmark metadata
pub fn run_worker(arguments: WorkerArguments) -> Result<(), Warning> {
    validate_worker_arguments(&arguments)?;
    if let Some(report) = crate::transports::baseline_ipc::run_worker(&arguments)? {
        print_report(arguments.role, report);
        return Ok(());
    }
    match arguments.role {
        WorkerRole::Publisher => {
            print_report(arguments.role, run_publisher(&arguments)?);
            Ok(())
        }
        WorkerRole::Subscriber => {
            print_report(arguments.role, run_subscriber(&arguments)?);
            Ok(())
        }
        WorkerRole::PingResponder => {
            print_report(arguments.role, run_ping_responder(&arguments)?);
            Ok(())
        }
        WorkerRole::PingRequester => {
            print_report(arguments.role, run_ping_requester(&arguments)?);
            Ok(())
        }
        WorkerRole::CrashSubscriber => run_crash_subscriber(&arguments),
        WorkerRole::CrashPublisher => run_crash_publisher(&arguments),
    }
}

pub(crate) fn warmup_messages_per_publisher(arguments: &WorkerArguments) -> usize {
    arguments.depth
}

fn run_publisher(arguments: &WorkerArguments) -> Result<WorkerReport, Warning> {
    let bus = Bus::connect(&arguments.domain)?;
    let mut publisher = bus.publisher(&arguments.topic, topic_config(arguments))?;
    let mut publish_bytes_buffer = vec![FILL_BYTE; arguments.payload_size];

    write_attach_ready_file(arguments)?;
    wait_warmup_start_file(arguments)?;
    for _message_index in 0..warmup_messages_per_publisher(arguments) {
        publish_one(arguments, &mut publisher, &mut publish_bytes_buffer, true)?;
    }
    if arguments.load_mode == LoadMode::Paced {
        prewarm_pacing_driver();
    }

    write_ready_file(arguments)?;
    let start_ns = wait_start_file(arguments)?;
    match arguments.load_mode {
        LoadMode::Saturated => {
            run_saturated_publisher(arguments, publisher, publish_bytes_buffer, start_ns)
        }
        LoadMode::Paced => {
            run_paced_publisher(arguments, publisher, publish_bytes_buffer, start_ns)
        }
    }
}

fn run_saturated_publisher(
    arguments: &WorkerArguments,
    mut publisher: Publisher,
    mut publish_bytes_buffer: Vec<u8>,
    start_ns: u64,
) -> Result<WorkerReport, Warning> {
    let system_start = ProcessSystemSnapshot::capture()?;
    let allocation_start = allocation::allocation_count();
    for _message_index in 0..arguments.messages_per_publisher {
        publish_one(arguments, &mut publisher, &mut publish_bytes_buffer, false)?;
    }
    let duration_ns = monotonic_time_ns()?.saturating_sub(start_ns);
    let allocations = allocation::allocation_delta(allocation_start);
    let system_delta = ProcessSystemSnapshot::capture()?.delta(system_start);
    let stats = publisher.stats();
    Ok(WorkerReport {
        messages: u64::from(arguments.messages_per_publisher),
        duration_ns,
        allocations,
        cpu_time_ns: system_delta.cpu_time_ns,
        context_switches: system_delta.context_switches,
        rss_bytes: system_delta.rss_bytes,
        pss_bytes: system_delta.pss_bytes,
        warnings: stats.warnings_total,
        skipped_messages: stats.skipped_messages,
        backpressure_pinned_slot: stats.backpressure_pinned_slot,
        publish_deadline_miss_count: 0,
    })
}

fn run_paced_publisher(
    arguments: &WorkerArguments,
    mut publisher: Publisher,
    mut publish_bytes_buffer: Vec<u8>,
    start_ns: u64,
) -> Result<WorkerReport, Warning> {
    let rate_hz = arguments.rate_hz.ok_or_else(invalid_argument)?;
    let schedule = PaceSchedule::new(start_ns, rate_hz);
    let spin_tail_ns = spin_tail_ns_from_us(arguments.pace_spin_tail_us)?;
    let miss_threshold_ns = schedule.period_ns()?;
    let sample_capacity = arguments.messages_per_publisher as usize;
    let mut publish_lateness = Vec::with_capacity(sample_capacity);
    let mut inter_publish_gaps = Vec::with_capacity(sample_capacity.saturating_sub(1));
    let system_start = ProcessSystemSnapshot::capture()?;
    let mut allocations = 0_u64;
    let mut previous_publish_ns = None;
    let mut publish_deadline_miss_count = 0_u64;
    for message_index in 0..arguments.messages_per_publisher {
        let deadline_ns = schedule.deadline_ns(message_index)?;
        let lateness_ns = wait_until(deadline_ns, spin_tail_ns)?;
        if lateness_ns > miss_threshold_ns {
            publish_deadline_miss_count = publish_deadline_miss_count.saturating_add(1);
        }
        publish_lateness.push(lateness_ns);
        let allocation_start = allocation::allocation_count();
        publish_one(arguments, &mut publisher, &mut publish_bytes_buffer, false)?;
        allocations = allocations.saturating_add(allocation::allocation_delta(allocation_start));
        let published_at_ns = monotonic_time_ns()?;
        if let Some(previous_publish_ns) = previous_publish_ns {
            inter_publish_gaps.push(published_at_ns.saturating_sub(previous_publish_ns));
        }
        previous_publish_ns = Some(published_at_ns);
    }
    let duration_ns = monotonic_time_ns()?.saturating_sub(start_ns);
    let system_delta = ProcessSystemSnapshot::capture()?.delta(system_start);
    print_prefixed_samples(PUBLISH_LATENESS_PREFIX, publish_lateness);
    print_prefixed_samples(INTER_PUBLISH_GAP_PREFIX, inter_publish_gaps);
    let stats = publisher.stats();
    Ok(WorkerReport {
        messages: u64::from(arguments.messages_per_publisher),
        duration_ns,
        allocations,
        cpu_time_ns: system_delta.cpu_time_ns,
        context_switches: system_delta.context_switches,
        rss_bytes: system_delta.rss_bytes,
        pss_bytes: system_delta.pss_bytes,
        warnings: stats.warnings_total,
        skipped_messages: stats.skipped_messages,
        backpressure_pinned_slot: stats.backpressure_pinned_slot,
        publish_deadline_miss_count,
    })
}

fn run_subscriber(arguments: &WorkerArguments) -> Result<WorkerReport, Warning> {
    let bus = Bus::connect(&arguments.domain)?;
    let subscriber = bus.subscriber(&arguments.topic)?;
    write_attach_ready_file(arguments)?;
    wait_warmup_start_file(arguments)?;
    let warmup_messages = u64::from(arguments.publisher_count)
        * u64::try_from(warmup_messages_per_publisher(arguments))
            .map_err(|_| invalid_argument())?;
    for _message_index in 0..warmup_messages {
        drop(receive_message(arguments.wait_mode, &subscriber)?);
    }
    prewarm_subscriber_streams(&subscriber)?;
    let published_messages_at_start = subscriber.stats().published_messages;
    write_ready_file(arguments)?;
    let start_ns = wait_start_file(arguments)?;

    let measured_messages =
        u64::from(arguments.publisher_count) * u64::from(arguments.messages_per_publisher);
    let mut transport_latencies = vec![0_u64; usize_from_u64(measured_messages)?];
    let mut end_to_end_latencies = vec![0_u64; usize_from_u64(measured_messages)?];
    let mut inter_receive_gaps = Vec::with_capacity(transport_latencies.len().saturating_sub(1));
    let mut backlog_samples = if arguments.transport.is_rbus() {
        Vec::with_capacity(transport_latencies.len())
    } else {
        Vec::new()
    };
    let drain_deadline_ns = paced_drain_deadline_ns(arguments, start_ns)?;
    let system_start = ProcessSystemSnapshot::capture()?;
    let allocation_start = match arguments.load_mode {
        LoadMode::Saturated => Some(allocation::allocation_count()),
        LoadMode::Paced => None,
    };
    let mut paced_allocations = 0_u64;
    let mut previous_receive_ns = None;
    let mut received_messages = 0_u64;
    for (transport_latency, end_to_end_latency) in transport_latencies
        .iter_mut()
        .zip(end_to_end_latencies.iter_mut())
    {
        let message = match arguments.load_mode {
            LoadMode::Saturated => receive_message(arguments.wait_mode, &subscriber)?,
            LoadMode::Paced => {
                let allocation_start = allocation::allocation_count();
                let message =
                    receive_message_until(arguments.wait_mode, &subscriber, drain_deadline_ns)?;
                paced_allocations = paced_allocations
                    .saturating_add(allocation::allocation_delta(allocation_start));
                message
            }
        };
        let timestamps = decode_timestamps(message.as_slice())?;
        let received_at = monotonic_time_ns()?;
        if let Some(previous_receive_ns) = previous_receive_ns {
            inter_receive_gaps.push(received_at.saturating_sub(previous_receive_ns));
        }
        previous_receive_ns = Some(received_at);
        *transport_latency = received_at.saturating_sub(timestamps.transport_ns);
        *end_to_end_latency = received_at.saturating_sub(timestamps.end_to_end_ns);
        received_messages = received_messages.saturating_add(1);
        if arguments.transport.is_rbus() {
            backlog_samples.push(
                subscriber
                    .stats()
                    .published_messages
                    .saturating_sub(published_messages_at_start)
                    .saturating_sub(received_messages),
            );
        }
        if arguments.slow_subscriber {
            std::thread::sleep(Duration::from_micros(SLOW_SUBSCRIBER_DELAY_MICROS));
        }
    }
    let duration_ns = monotonic_time_ns()?.saturating_sub(start_ns);
    let allocations = match allocation_start {
        Some(allocation_start) => allocation::allocation_delta(allocation_start),
        None => paced_allocations,
    };
    let system_delta = ProcessSystemSnapshot::capture()?.delta(system_start);
    print_one_way_latencies(transport_latencies, end_to_end_latencies);
    print_prefixed_samples(INTER_RECEIVE_GAP_PREFIX, inter_receive_gaps);
    print_prefixed_samples(BACKLOG_PREFIX, backlog_samples);
    let stats = subscriber.stats();
    Ok(WorkerReport {
        messages: measured_messages,
        duration_ns,
        allocations,
        cpu_time_ns: system_delta.cpu_time_ns,
        context_switches: system_delta.context_switches,
        rss_bytes: system_delta.rss_bytes,
        pss_bytes: system_delta.pss_bytes,
        warnings: stats.warnings_total,
        skipped_messages: stats.skipped_messages,
        backpressure_pinned_slot: stats.backpressure_pinned_slot,
        publish_deadline_miss_count: 0,
    })
}

fn run_ping_responder(arguments: &WorkerArguments) -> Result<WorkerReport, Warning> {
    let bus = Bus::connect(&arguments.domain)?;
    let subscriber = bus.subscriber(&arguments.topic)?;
    let mut publisher = bus.publisher(&arguments.response_topic, topic_config(arguments))?;
    write_attach_ready_file(arguments)?;
    wait_warmup_start_file(arguments)?;
    let mut publish_bytes_buffer = vec![FILL_BYTE; arguments.payload_size];
    for _message_index in 0..warmup_messages_per_publisher(arguments) {
        let message = receive_message(arguments.wait_mode, &subscriber)?;
        let sent_at = decode_timestamps(message.as_slice())?.end_to_end_ns;
        publish_timestamped(
            arguments,
            &mut publisher,
            &mut publish_bytes_buffer,
            sent_at,
            true,
        )?;
    }
    write_ready_file(arguments)?;
    wait_start_file(arguments)?;
    let total_messages = u64::from(arguments.messages_per_publisher);
    let system_start = ProcessSystemSnapshot::capture()?;
    let allocation_start = allocation::allocation_count();
    let started_at = monotonic_time_ns()?;
    for _message_index in 0..total_messages {
        let message = receive_message(arguments.wait_mode, &subscriber)?;
        let sent_at = decode_timestamps(message.as_slice())?.end_to_end_ns;
        publish_timestamped(
            arguments,
            &mut publisher,
            &mut publish_bytes_buffer,
            sent_at,
            false,
        )?;
    }
    let duration_ns = monotonic_time_ns()?.saturating_sub(started_at);
    let allocations = allocation::allocation_delta(allocation_start);
    let system_delta = ProcessSystemSnapshot::capture()?.delta(system_start);
    let publisher_stats = publisher.stats();
    let subscriber_stats = subscriber.stats();
    Ok(WorkerReport {
        messages: total_messages,
        duration_ns,
        allocations,
        cpu_time_ns: system_delta.cpu_time_ns,
        context_switches: system_delta.context_switches,
        rss_bytes: system_delta.rss_bytes,
        pss_bytes: system_delta.pss_bytes,
        warnings: publisher_stats
            .warnings_total
            .saturating_add(subscriber_stats.warnings_total),
        skipped_messages: subscriber_stats.skipped_messages,
        backpressure_pinned_slot: publisher_stats.backpressure_pinned_slot,
        publish_deadline_miss_count: 0,
    })
}

fn run_ping_requester(arguments: &WorkerArguments) -> Result<WorkerReport, Warning> {
    let bus = Bus::connect(&arguments.domain)?;
    let mut publisher = bus.publisher(&arguments.topic, topic_config(arguments))?;
    let subscriber = bus.subscriber(&arguments.response_topic)?;
    let mut publish_bytes_buffer = vec![FILL_BYTE; arguments.payload_size];
    write_attach_ready_file(arguments)?;
    wait_warmup_start_file(arguments)?;
    for _message_index in 0..warmup_messages_per_publisher(arguments) {
        let sent_at = monotonic_time_ns()?;
        publish_timestamped(
            arguments,
            &mut publisher,
            &mut publish_bytes_buffer,
            sent_at,
            true,
        )?;
        drop(receive_message(arguments.wait_mode, &subscriber)?);
    }
    write_ready_file(arguments)?;
    wait_start_file(arguments)?;
    let mut latencies = vec![0_u64; arguments.messages_per_publisher as usize];
    let system_start = ProcessSystemSnapshot::capture()?;
    let allocation_start = allocation::allocation_count();
    let started_at = monotonic_time_ns()?;
    for latency in &mut latencies {
        let sent_at = monotonic_time_ns()?;
        publish_timestamped(
            arguments,
            &mut publisher,
            &mut publish_bytes_buffer,
            sent_at,
            false,
        )?;
        let response = receive_message(arguments.wait_mode, &subscriber)?;
        let response_timestamp = decode_timestamps(response.as_slice())?.end_to_end_ns;
        *latency = monotonic_time_ns()?.saturating_sub(response_timestamp);
    }
    let duration_ns = monotonic_time_ns()?.saturating_sub(started_at);
    let allocations = allocation::allocation_delta(allocation_start);
    let system_delta = ProcessSystemSnapshot::capture()?.delta(system_start);
    for latency in latencies {
        println!("{ROUND_TRIP_PREFIX}{latency}");
    }
    let publisher_stats = publisher.stats();
    let subscriber_stats = subscriber.stats();
    Ok(WorkerReport {
        messages: u64::from(arguments.messages_per_publisher),
        duration_ns,
        allocations,
        cpu_time_ns: system_delta.cpu_time_ns,
        context_switches: system_delta.context_switches,
        rss_bytes: system_delta.rss_bytes,
        pss_bytes: system_delta.pss_bytes,
        warnings: publisher_stats
            .warnings_total
            .saturating_add(subscriber_stats.warnings_total),
        skipped_messages: subscriber_stats.skipped_messages,
        backpressure_pinned_slot: publisher_stats.backpressure_pinned_slot,
        publish_deadline_miss_count: 0,
    })
}

fn run_crash_subscriber(arguments: &WorkerArguments) -> Result<(), Warning> {
    let bus = Bus::connect(&arguments.domain)?;
    let subscriber = bus.subscriber(&arguments.topic)?;
    write_attach_ready_file(arguments)?;
    write_ready_file(arguments)?;
    wait_start_file(arguments)?;
    let _message = receive_message(arguments.wait_mode, &subscriber)?;
    std::process::abort();
}

fn run_crash_publisher(arguments: &WorkerArguments) -> Result<(), Warning> {
    let bus = Bus::connect(&arguments.domain)?;
    let mut publisher = bus.publisher(&arguments.topic, topic_config(arguments))?;
    let mut publish_bytes_buffer = vec![FILL_BYTE; arguments.payload_size];
    let total_messages = usize::try_from(arguments.messages_per_publisher)
        .map_err(|_| invalid_argument())?
        .saturating_add(warmup_messages_per_publisher(arguments));
    for _message_index in 0..total_messages {
        publish_one(arguments, &mut publisher, &mut publish_bytes_buffer, true)?;
    }
    std::process::abort();
}

fn publish_one(
    arguments: &WorkerArguments,
    publisher: &mut Publisher,
    publish_bytes_buffer: &mut [u8],
    force_payload_fill: bool,
) -> Result<(), Warning> {
    let end_to_end_ns =
        if arguments.transport.fills_payload_in_measured_window() || force_payload_fill {
            monotonic_time_ns()?
        } else {
            UNSET_TIMESTAMP_NS
        };
    publish_timestamped(
        arguments,
        publisher,
        publish_bytes_buffer,
        end_to_end_ns,
        force_payload_fill,
    )
}

fn publish_timestamped(
    arguments: &WorkerArguments,
    publisher: &mut Publisher,
    publish_bytes_buffer: &mut [u8],
    end_to_end_ns: u64,
    force_payload_fill: bool,
) -> Result<(), Warning> {
    match arguments.transport {
        TransportKind::RbusLoanedMetadataOnly | TransportKind::RbusLoanedFill => {
            let mut message = publisher.loan(arguments.payload_size)?;
            let payload = message.as_mut_slice();
            write_payload_timestamps(arguments, payload, end_to_end_ns, force_payload_fill)?;
            let _sequence = message.publish()?;
        }
        TransportKind::RbusPublishBytes => {
            write_payload_timestamps(
                arguments,
                publish_bytes_buffer,
                end_to_end_ns,
                force_payload_fill,
            )?;
            let _sequence = publisher.publish_bytes(publish_bytes_buffer)?;
        }
        _ => return Err(invalid_argument()),
    }
    Ok(())
}

fn write_payload_timestamps(
    arguments: &WorkerArguments,
    payload: &mut [u8],
    end_to_end_ns: u64,
    force_payload_fill: bool,
) -> Result<(), Warning> {
    let fills_payload =
        arguments.transport.fills_payload_in_measured_window() || force_payload_fill;
    if fills_payload {
        payload.fill(FILL_BYTE);
    }
    let transport_ns = monotonic_time_ns()?;
    let end_to_end_ns = if fills_payload || end_to_end_ns != UNSET_TIMESTAMP_NS {
        end_to_end_ns
    } else {
        transport_ns
    };
    encode_timestamps(
        payload,
        PayloadTimestamps {
            transport_ns,
            end_to_end_ns,
        },
    )
}

fn receive_message(
    wait_mode: WaitMode,
    subscriber: &Subscriber,
) -> Result<rbus::MessageRef<'_>, Warning> {
    match wait_mode {
        WaitMode::BusyPoll => loop {
            if let Some(message) = subscriber.try_recv()? {
                return Ok(message);
            }
        },
        WaitMode::Recv => subscriber.recv(),
        WaitMode::RecvTimeout => subscriber
            .recv_timeout(Duration::from_millis(RECV_TIMEOUT_MS))?
            .ok_or_else(|| {
                Warning::new(WarningCode::Timeout, COMPONENT_RBUS_BENCH, "recv_timeout")
            }),
    }
}

fn receive_message_until(
    wait_mode: WaitMode,
    subscriber: &Subscriber,
    deadline_ns: u64,
) -> Result<rbus::MessageRef<'_>, Warning> {
    match wait_mode {
        WaitMode::BusyPoll => loop {
            if let Some(message) = subscriber.try_recv()? {
                return Ok(message);
            }
            if monotonic_time_ns()? > deadline_ns {
                return Err(Warning::new(
                    WarningCode::Timeout,
                    COMPONENT_RBUS_BENCH,
                    "paced_receive_drain",
                ));
            }
            core::hint::spin_loop();
        },
        WaitMode::Recv | WaitMode::RecvTimeout => receive_message(wait_mode, subscriber),
    }
}

fn paced_drain_deadline_ns(arguments: &WorkerArguments, start_ns: u64) -> Result<u64, Warning> {
    if arguments.load_mode == LoadMode::Saturated {
        return Ok(u64::MAX);
    }
    let rate_hz = arguments.rate_hz.ok_or_else(invalid_argument)?;
    let schedule = PaceSchedule::new(start_ns, rate_hz);
    let last_message_index = arguments.messages_per_publisher.saturating_sub(1);
    schedule
        .deadline_ns(last_message_index)?
        .checked_add(schedule.period_ns()?)
        .and_then(|deadline| {
            deadline.checked_add(Duration::from_millis(PACED_DRAIN_TIMEOUT_MS).as_nanos() as u64)
        })
        .ok_or_else(invalid_argument)
}

fn topic_config(arguments: &WorkerArguments) -> TopicConfig {
    TopicConfig::new(arguments.payload_size, arguments.depth)
}

fn prewarm_pacing_driver() {
    std::thread::sleep(Duration::from_nanos(PACING_PREWARM_SLEEP_NS));
}

fn prewarm_subscriber_streams(subscriber: &Subscriber) -> Result<(), Warning> {
    while subscriber.try_recv()?.is_some() {}
    Ok(())
}
