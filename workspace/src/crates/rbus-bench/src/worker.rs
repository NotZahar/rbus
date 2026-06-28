use std::path::PathBuf;
use std::time::Duration;

use rbus::{Bus, Publisher, Subscriber, TopicConfig};
use rbus_core::warning::{Warning, WarningCode};

use crate::allocation;
use crate::system_metrics::{ProcessSystemSnapshot, monotonic_time_ns};
use crate::transport_kind::TransportKind;
use crate::wait_mode::WaitMode;

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const BLOCKING_PUBLISH_DELAY_MICROS: u64 = 100;
const BUSY_POLL_PUBLISHER_START_DELAY_MS: u64 = 1_000;
const FILL_BYTE: u8 = 0xA5;
const BUSY_POLL_PUBLISHER_STREAM_LINGER_MS: u64 = 500;
const READY_FILE_BYTES: &[u8] = b"ready\n";
const RECV_TIMEOUT_MS: u64 = 5_000;
const TIMESTAMP_BYTES: usize = core::mem::size_of::<u64>();
const WARMUP_MESSAGES_PER_PUBLISHER: u32 = 2;

pub const LATENCY_PREFIX: &str = "RBUS_BENCH_LATENCY_NS ";
pub const RESULT_PREFIX: &str = "RBUS_BENCH_WORKER_RESULT ";

const _: () = assert!(TIMESTAMP_BYTES <= 64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerRole {
    Publisher,
    Subscriber,
    PingResponder,
    PingRequester,
    CrashSubscriber,
    CrashPublisher,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerArguments {
    pub role: WorkerRole,
    pub domain: String,
    pub topic: String,
    pub response_topic: String,
    pub payload_size: usize,
    pub depth: usize,
    pub worker_index: u8,
    pub publisher_count: u8,
    pub subscriber_count: u8,
    pub messages_per_publisher: u32,
    pub wait_mode: WaitMode,
    pub transport: TransportKind,
    pub ready_file: Option<PathBuf>,
    pub slow_subscriber: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkerReport {
    pub messages: u64,
    pub duration_ns: u64,
    pub allocations: u64,
    pub cpu_time_ns: u64,
    pub context_switches: u64,
    pub rss_bytes: u64,
    pub pss_bytes: u64,
    pub warnings: u64,
    pub skipped_messages: u64,
    pub backpressure_pinned_slot: u64,
}

impl WorkerRole {
    /// CLI metadata
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "publisher" => Some(Self::Publisher),
            "subscriber" => Some(Self::Subscriber),
            "ping-responder" => Some(Self::PingResponder),
            "ping-requester" => Some(Self::PingRequester),
            "crash-subscriber" => Some(Self::CrashSubscriber),
            "crash-publisher" => Some(Self::CrashPublisher),
            _ => None,
        }
    }

    /// CLI metadata
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Publisher => "publisher",
            Self::Subscriber => "subscriber",
            Self::PingResponder => "ping-responder",
            Self::PingRequester => "ping-requester",
            Self::CrashSubscriber => "crash-subscriber",
            Self::CrashPublisher => "crash-publisher",
        }
    }
}

impl WorkerArguments {
    /// Benchmark metadata
    pub fn parse<I>(role: WorkerRole, arguments: I) -> Result<Self, Warning>
    where
        I: IntoIterator<Item = String>,
    {
        let mut parsed = WorkerArgumentBuilder::new(role);
        let mut iterator = arguments.into_iter();
        while let Some(argument) = iterator.next() {
            match argument.as_str() {
                "--domain" => parsed.domain = Some(next_argument(&mut iterator)?),
                "--topic" => parsed.topic = Some(next_argument(&mut iterator)?),
                "--response-topic" => parsed.response_topic = Some(next_argument(&mut iterator)?),
                "--payload-size" => parsed.payload_size = Some(parse_next(&mut iterator)?),
                "--depth" => parsed.depth = Some(parse_next(&mut iterator)?),
                "--worker-index" => parsed.worker_index = Some(parse_next(&mut iterator)?),
                "--publisher-count" => parsed.publisher_count = Some(parse_next(&mut iterator)?),
                "--subscriber-count" => parsed.subscriber_count = Some(parse_next(&mut iterator)?),
                "--messages" => parsed.messages_per_publisher = Some(parse_next(&mut iterator)?),
                "--wait-mode" => {
                    let value = next_argument(&mut iterator)?;
                    parsed.wait_mode = Some(WaitMode::parse(&value).ok_or_else(invalid_argument)?);
                }
                "--transport" => {
                    let value = next_argument(&mut iterator)?;
                    parsed.transport =
                        Some(TransportKind::parse(&value).ok_or_else(invalid_argument)?);
                }
                "--ready-file" => {
                    parsed.ready_file = Some(PathBuf::from(next_argument(&mut iterator)?))
                }
                "--slow-subscriber" => parsed.slow_subscriber = true,
                _ => return Err(invalid_argument()),
            }
        }
        parsed.build()
    }
}

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

pub(crate) const fn warmup_messages_per_publisher() -> u32 {
    WARMUP_MESSAGES_PER_PUBLISHER
}

fn run_publisher(arguments: &WorkerArguments) -> Result<WorkerReport, Warning> {
    let bus = Bus::connect(&arguments.domain)?;
    let publisher = bus.publisher(&arguments.topic, topic_config(arguments))?;
    if arguments.transport.is_rbus() && arguments.wait_mode == WaitMode::BusyPoll {
        std::thread::sleep(Duration::from_millis(BUSY_POLL_PUBLISHER_START_DELAY_MS));
    }
    let mut publish_bytes_buffer = vec![FILL_BYTE; arguments.payload_size];

    for _message_index in 0..WARMUP_MESSAGES_PER_PUBLISHER {
        publish_one(arguments, &publisher, &mut publish_bytes_buffer)?;
    }

    let system_start = ProcessSystemSnapshot::capture()?;
    let allocation_start = allocation::allocation_count();
    let started_at = monotonic_time_ns()?;
    for _message_index in 0..arguments.messages_per_publisher {
        if arguments.wait_mode.uses_blocking_wait() {
            std::thread::sleep(Duration::from_micros(BLOCKING_PUBLISH_DELAY_MICROS));
        }
        publish_one(arguments, &publisher, &mut publish_bytes_buffer)?;
    }
    let duration_ns = monotonic_time_ns()?.saturating_sub(started_at);
    let allocations = allocation::allocation_delta(allocation_start);
    let system_delta = ProcessSystemSnapshot::capture()?.delta(system_start);
    let stats = publisher.stats();
    if arguments.transport.is_rbus() && arguments.wait_mode == WaitMode::BusyPoll {
        std::thread::sleep(Duration::from_millis(BUSY_POLL_PUBLISHER_STREAM_LINGER_MS));
    }
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
    write_ready_file(arguments)?;
    let warmup_messages =
        u64::from(arguments.publisher_count) * u64::from(WARMUP_MESSAGES_PER_PUBLISHER);
    for _message_index in 0..warmup_messages {
        drop(receive_message(arguments.wait_mode, &subscriber)?);
    }

    let measured_messages =
        u64::from(arguments.publisher_count) * u64::from(arguments.messages_per_publisher);
    let mut latencies = vec![0_u64; usize_from_u64(measured_messages)?];
    let system_start = ProcessSystemSnapshot::capture()?;
    let allocation_start = allocation::allocation_count();
    let started_at = monotonic_time_ns()?;
    for latency in &mut latencies {
        let message = receive_message(arguments.wait_mode, &subscriber)?;
        let sent_at = decode_timestamp(message.as_slice())?;
        *latency = monotonic_time_ns()?.saturating_sub(sent_at);
        if arguments.slow_subscriber {
            std::thread::sleep(Duration::from_micros(BLOCKING_PUBLISH_DELAY_MICROS));
        }
    }
    let duration_ns = monotonic_time_ns()?.saturating_sub(started_at);
    let allocations = allocation::allocation_delta(allocation_start);
    let system_delta = ProcessSystemSnapshot::capture()?.delta(system_start);
    for latency in latencies {
        println!("{LATENCY_PREFIX}{latency}");
    }
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
    let publisher = bus.publisher(&arguments.response_topic, topic_config(arguments))?;
    write_ready_file(arguments)?;
    let mut publish_bytes_buffer = vec![FILL_BYTE; arguments.payload_size];
    let total_messages = u64::from(arguments.messages_per_publisher);
    let system_start = ProcessSystemSnapshot::capture()?;
    let allocation_start = allocation::allocation_count();
    let started_at = monotonic_time_ns()?;
    for _message_index in 0..total_messages {
        let message = receive_message(arguments.wait_mode, &subscriber)?;
        let sent_at = decode_timestamp(message.as_slice())?;
        publish_timestamped(arguments, &publisher, &mut publish_bytes_buffer, sent_at)?;
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
    let publisher = bus.publisher(&arguments.topic, topic_config(arguments))?;
    let subscriber = bus.subscriber(&arguments.response_topic)?;
    let mut publish_bytes_buffer = vec![FILL_BYTE; arguments.payload_size];
    let mut latencies = vec![0_u64; arguments.messages_per_publisher as usize];
    let system_start = ProcessSystemSnapshot::capture()?;
    let allocation_start = allocation::allocation_count();
    let started_at = monotonic_time_ns()?;
    for latency in &mut latencies {
        let sent_at = monotonic_time_ns()?;
        publish_timestamped(arguments, &publisher, &mut publish_bytes_buffer, sent_at)?;
        let response = receive_message(arguments.wait_mode, &subscriber)?;
        let response_timestamp = decode_timestamp(response.as_slice())?;
        *latency = monotonic_time_ns()?.saturating_sub(response_timestamp);
    }
    let duration_ns = monotonic_time_ns()?.saturating_sub(started_at);
    let allocations = allocation::allocation_delta(allocation_start);
    let system_delta = ProcessSystemSnapshot::capture()?.delta(system_start);
    for latency in latencies {
        println!("{LATENCY_PREFIX}{latency}");
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
    write_ready_file(arguments)?;
    let _message = receive_message(arguments.wait_mode, &subscriber)?;
    std::process::abort();
}

fn run_crash_publisher(arguments: &WorkerArguments) -> Result<(), Warning> {
    let bus = Bus::connect(&arguments.domain)?;
    let publisher = bus.publisher(&arguments.topic, topic_config(arguments))?;
    let mut publish_bytes_buffer = vec![FILL_BYTE; arguments.payload_size];
    let total_messages = arguments
        .messages_per_publisher
        .saturating_add(WARMUP_MESSAGES_PER_PUBLISHER);
    for _message_index in 0..total_messages {
        publish_one(arguments, &publisher, &mut publish_bytes_buffer)?;
    }
    std::process::abort();
}

fn publish_one(
    arguments: &WorkerArguments,
    publisher: &Publisher,
    publish_bytes_buffer: &mut [u8],
) -> Result<(), Warning> {
    let sent_at = monotonic_time_ns()?;
    publish_timestamped(arguments, publisher, publish_bytes_buffer, sent_at)
}

fn publish_timestamped(
    arguments: &WorkerArguments,
    publisher: &Publisher,
    publish_bytes_buffer: &mut [u8],
    timestamp_ns: u64,
) -> Result<(), Warning> {
    match arguments.transport {
        TransportKind::RbusLoaned => {
            let mut message = publisher.loan(arguments.payload_size)?;
            let payload = message.as_mut_slice();
            payload.fill(FILL_BYTE);
            encode_timestamp(payload, timestamp_ns)?;
            let _sequence = publisher.publish(message)?;
        }
        TransportKind::RbusPublishBytes => {
            publish_bytes_buffer.fill(FILL_BYTE);
            encode_timestamp(publish_bytes_buffer, timestamp_ns)?;
            let _sequence = publisher.publish_bytes(publish_bytes_buffer)?;
        }
        _ => return Err(invalid_argument()),
    }
    Ok(())
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

fn encode_timestamp(payload: &mut [u8], timestamp_ns: u64) -> Result<(), Warning> {
    let target = payload.get_mut(..TIMESTAMP_BYTES).ok_or_else(|| {
        Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_RBUS_BENCH,
            "encode_timestamp",
        )
    })?;
    target.copy_from_slice(&timestamp_ns.to_ne_bytes());
    Ok(())
}

fn decode_timestamp(payload: &[u8]) -> Result<u64, Warning> {
    let source = payload.get(..TIMESTAMP_BYTES).ok_or_else(|| {
        Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_RBUS_BENCH,
            "decode_timestamp",
        )
    })?;
    let mut bytes = [0_u8; TIMESTAMP_BYTES];
    bytes.copy_from_slice(source);
    Ok(u64::from_ne_bytes(bytes))
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

fn parse_next<T, I>(iterator: &mut I) -> Result<T, Warning>
where
    T: core::str::FromStr,
    I: Iterator<Item = String>,
{
    next_argument(iterator)?
        .parse::<T>()
        .map_err(|_| invalid_argument())
}

fn next_argument<I>(iterator: &mut I) -> Result<String, Warning>
where
    I: Iterator<Item = String>,
{
    iterator.next().ok_or_else(invalid_argument)
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

struct WorkerArgumentBuilder {
    role: WorkerRole,
    domain: Option<String>,
    topic: Option<String>,
    response_topic: Option<String>,
    payload_size: Option<usize>,
    depth: Option<usize>,
    worker_index: Option<u8>,
    publisher_count: Option<u8>,
    subscriber_count: Option<u8>,
    messages_per_publisher: Option<u32>,
    wait_mode: Option<WaitMode>,
    transport: Option<TransportKind>,
    ready_file: Option<PathBuf>,
    slow_subscriber: bool,
}

impl WorkerArgumentBuilder {
    fn new(role: WorkerRole) -> Self {
        Self {
            role,
            domain: None,
            topic: None,
            response_topic: Some(String::new()),
            payload_size: None,
            depth: None,
            worker_index: None,
            publisher_count: None,
            subscriber_count: None,
            messages_per_publisher: None,
            wait_mode: None,
            transport: None,
            ready_file: None,
            slow_subscriber: false,
        }
    }

    fn build(self) -> Result<WorkerArguments, Warning> {
        Ok(WorkerArguments {
            role: self.role,
            domain: self.domain.ok_or_else(invalid_argument)?,
            topic: self.topic.ok_or_else(invalid_argument)?,
            response_topic: self.response_topic.ok_or_else(invalid_argument)?,
            payload_size: self.payload_size.ok_or_else(invalid_argument)?,
            depth: self.depth.ok_or_else(invalid_argument)?,
            worker_index: self.worker_index.ok_or_else(invalid_argument)?,
            publisher_count: self.publisher_count.ok_or_else(invalid_argument)?,
            subscriber_count: self.subscriber_count.ok_or_else(invalid_argument)?,
            messages_per_publisher: self.messages_per_publisher.ok_or_else(invalid_argument)?,
            wait_mode: self.wait_mode.ok_or_else(invalid_argument)?,
            transport: self.transport.ok_or_else(invalid_argument)?,
            ready_file: self.ready_file,
            slow_subscriber: self.slow_subscriber,
        })
    }
}
