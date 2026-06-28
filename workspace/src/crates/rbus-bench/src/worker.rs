use std::time::Duration;

use rbus::{Bus, Publisher, Subscriber, TopicConfig};
use rbus_core::warning::{Warning, WarningCode};

use crate::allocation;
use crate::system_metrics::{ProcessSystemSnapshot, monotonic_time_ns};
use crate::timestamp_payload::{
    PayloadTimestamps, TIMESTAMP_BYTES, decode_timestamps, encode_timestamps,
};
use crate::transport_kind::TransportKind;
use crate::wait_mode::WaitMode;
pub use crate::worker_args::{WorkerArguments, WorkerReport, WorkerRole};

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const FILL_BYTE: u8 = 0xA5;
const SLOW_SUBSCRIBER_DELAY_MICROS: u64 = 100;
const READY_FILE_BYTES: &[u8] = b"ready\n";
const RECV_TIMEOUT_MS: u64 = 5_000;
const START_WAIT_POLL_INTERVAL_MS: u64 = 1;
const START_WAIT_TIMEOUT_MS: u64 = 300_000;
const UNSET_TIMESTAMP_NS: u64 = 0;

pub const TRANSPORT_LATENCY_PREFIX: &str = "RBUS_BENCH_TRANSPORT_LATENCY_NS ";
pub const END_TO_END_LATENCY_PREFIX: &str = "RBUS_BENCH_END_TO_END_LATENCY_NS ";
pub const ROUND_TRIP_PREFIX: &str = "RBUS_BENCH_ROUND_TRIP_NS ";
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

    write_ready_file(arguments)?;
    wait_start_file(arguments)?;
    let system_start = ProcessSystemSnapshot::capture()?;
    let allocation_start = allocation::allocation_count();
    let started_at = monotonic_time_ns()?;
    for _message_index in 0..arguments.messages_per_publisher {
        publish_one(arguments, &mut publisher, &mut publish_bytes_buffer, false)?;
    }
    let duration_ns = monotonic_time_ns()?.saturating_sub(started_at);
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
    write_ready_file(arguments)?;
    wait_start_file(arguments)?;

    let measured_messages =
        u64::from(arguments.publisher_count) * u64::from(arguments.messages_per_publisher);
    let mut transport_latencies = vec![0_u64; usize_from_u64(measured_messages)?];
    let mut end_to_end_latencies = vec![0_u64; usize_from_u64(measured_messages)?];
    let system_start = ProcessSystemSnapshot::capture()?;
    let allocation_start = allocation::allocation_count();
    let started_at = monotonic_time_ns()?;
    for (transport_latency, end_to_end_latency) in transport_latencies
        .iter_mut()
        .zip(end_to_end_latencies.iter_mut())
    {
        let message = receive_message(arguments.wait_mode, &subscriber)?;
        let timestamps = decode_timestamps(message.as_slice())?;
        let received_at = monotonic_time_ns()?;
        *transport_latency = received_at.saturating_sub(timestamps.transport_ns);
        *end_to_end_latency = received_at.saturating_sub(timestamps.end_to_end_ns);
        if arguments.slow_subscriber {
            std::thread::sleep(Duration::from_micros(SLOW_SUBSCRIBER_DELAY_MICROS));
        }
    }
    let duration_ns = monotonic_time_ns()?.saturating_sub(started_at);
    let allocations = allocation::allocation_delta(allocation_start);
    let system_delta = ProcessSystemSnapshot::capture()?.delta(system_start);
    print_one_way_latencies(transport_latencies, end_to_end_latencies);
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

fn topic_config(arguments: &WorkerArguments) -> TopicConfig {
    TopicConfig::new(arguments.payload_size, arguments.depth)
}

fn write_ready_file(arguments: &WorkerArguments) -> Result<(), Warning> {
    match &arguments.ready_file {
        Some(path) => std::fs::write(path, READY_FILE_BYTES)
            .map_err(|error| warning_from_io(error, "write_ready_file")),
        None => Ok(()),
    }
}

fn write_attach_ready_file(arguments: &WorkerArguments) -> Result<(), Warning> {
    match &arguments.attach_ready_file {
        Some(path) => std::fs::write(path, READY_FILE_BYTES)
            .map_err(|error| warning_from_io(error, "write_attach_ready_file")),
        None => Ok(()),
    }
}

fn wait_start_file(arguments: &WorkerArguments) -> Result<(), Warning> {
    wait_flag_file(&arguments.start_file)
}

fn wait_warmup_start_file(arguments: &WorkerArguments) -> Result<(), Warning> {
    wait_flag_file(&arguments.warmup_start_file)
}

fn wait_flag_file(path: &Option<std::path::PathBuf>) -> Result<(), Warning> {
    let Some(path) = path else {
        return Ok(());
    };
    let started_at = monotonic_time_ns()?;
    loop {
        if path.exists() {
            return Ok(());
        }
        if monotonic_time_ns()?.saturating_sub(started_at)
            > Duration::from_millis(START_WAIT_TIMEOUT_MS).as_nanos() as u64
        {
            return Err(invalid_argument());
        }
        std::thread::sleep(Duration::from_millis(START_WAIT_POLL_INTERVAL_MS));
    }
}

fn print_one_way_latencies(transport_latencies: Vec<u64>, end_to_end_latencies: Vec<u64>) {
    for latency in transport_latencies {
        println!("{TRANSPORT_LATENCY_PREFIX}{latency}");
    }
    for latency in end_to_end_latencies {
        println!("{END_TO_END_LATENCY_PREFIX}{latency}");
    }
}

fn print_report(role: WorkerRole, report: WorkerReport) {
    println!(
        concat!(
            "{}role={} messages={} duration_ns={} allocations={} ",
            "cpu_time_ns={} context_switches={} rss_bytes={} pss_bytes={} ",
            "warnings={} skipped_messages={} backpressure_pinned_slot={}"
        ),
        RESULT_PREFIX,
        role.as_str(),
        report.messages,
        report.duration_ns,
        report.allocations,
        report.cpu_time_ns,
        report.context_switches,
        report.rss_bytes,
        report.pss_bytes,
        report.warnings,
        report.skipped_messages,
        report.backpressure_pinned_slot
    );
}

fn validate_worker_arguments(arguments: &WorkerArguments) -> Result<(), Warning> {
    if arguments.domain.is_empty()
        || arguments.topic.is_empty()
        || arguments.payload_size < TIMESTAMP_BYTES
        || arguments.depth == 0
        || !arguments.depth.is_power_of_two()
        || arguments.worker_index >= arguments.publisher_count.max(arguments.subscriber_count)
        || arguments.publisher_count == 0
        || arguments.subscriber_count == 0
        || arguments.messages_per_publisher == 0
    {
        return Err(invalid_argument());
    }
    Ok(())
}

fn usize_from_u64(value: u64) -> Result<usize, Warning> {
    usize::try_from(value).map_err(|_| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_RBUS_BENCH,
            "usize_from_u64",
        )
    })
}

fn invalid_argument() -> Warning {
    Warning::new(
        WarningCode::InvalidConfig,
        COMPONENT_RBUS_BENCH,
        "WorkerArguments::parse",
    )
}

fn warning_from_io(error: std::io::Error, operation: &'static str) -> Warning {
    match error.raw_os_error() {
        Some(errno) => Warning::new(WarningCode::SyscallFailed, COMPONENT_RBUS_BENCH, operation)
            .with_os_errno(errno),
        None => Warning::new(WarningCode::SyscallFailed, COMPONENT_RBUS_BENCH, operation),
    }
}
