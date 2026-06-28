use std::path::PathBuf;

use rbus_core::warning::{Warning, WarningCode};

use crate::load_mode::{LoadMode, parse_positive_rate_hz};
use crate::transport_kind::TransportKind;
use crate::wait_mode::WaitMode;

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerRole {
    Publisher,
    Subscriber,
    PingResponder,
    PingRequester,
    CrashSubscriber,
    CrashPublisher,
}

#[derive(Clone, Debug, PartialEq)]
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
    pub load_mode: LoadMode,
    pub rate_hz: Option<f64>,
    pub pace_spin_tail_us: u64,
    pub wait_mode: WaitMode,
    pub transport: TransportKind,
    pub attach_ready_file: Option<PathBuf>,
    pub warmup_start_file: Option<PathBuf>,
    pub ready_file: Option<PathBuf>,
    pub start_file: Option<PathBuf>,
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
    pub publish_deadline_miss_count: u64,
}

impl WorkerRole {
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
                "--load-mode" => {
                    let value = next_argument(&mut iterator)?;
                    parsed.load_mode = Some(LoadMode::parse(&value).ok_or_else(invalid_argument)?);
                }
                "--rate-hz" => {
                    let value = next_argument(&mut iterator)?;
                    parsed.rate_hz = Some(parse_positive_rate_hz(&value)?);
                }
                "--pace-spin-tail-us" => {
                    parsed.pace_spin_tail_us = Some(parse_next(&mut iterator)?);
                }
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
                    parsed.ready_file = Some(PathBuf::from(next_argument(&mut iterator)?));
                }
                "--attach-ready-file" => {
                    parsed.attach_ready_file = Some(PathBuf::from(next_argument(&mut iterator)?));
                }
                "--warmup-start-file" => {
                    parsed.warmup_start_file = Some(PathBuf::from(next_argument(&mut iterator)?));
                }
                "--start-file" => {
                    parsed.start_file = Some(PathBuf::from(next_argument(&mut iterator)?));
                }
                "--slow-subscriber" => parsed.slow_subscriber = true,
                _ => return Err(invalid_argument()),
            }
        }
        parsed.build()
    }
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
    load_mode: Option<LoadMode>,
    rate_hz: Option<f64>,
    pace_spin_tail_us: Option<u64>,
    wait_mode: Option<WaitMode>,
    transport: Option<TransportKind>,
    attach_ready_file: Option<PathBuf>,
    warmup_start_file: Option<PathBuf>,
    ready_file: Option<PathBuf>,
    start_file: Option<PathBuf>,
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
            load_mode: Some(LoadMode::Saturated),
            rate_hz: None,
            pace_spin_tail_us: Some(0),
            wait_mode: None,
            transport: None,
            attach_ready_file: None,
            warmup_start_file: None,
            ready_file: None,
            start_file: None,
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
            load_mode: self.load_mode.ok_or_else(invalid_argument)?,
            rate_hz: self.rate_hz,
            pace_spin_tail_us: self.pace_spin_tail_us.ok_or_else(invalid_argument)?,
            wait_mode: self.wait_mode.ok_or_else(invalid_argument)?,
            transport: self.transport.ok_or_else(invalid_argument)?,
            attach_ready_file: self.attach_ready_file,
            warmup_start_file: self.warmup_start_file,
            ready_file: self.ready_file,
            start_file: self.start_file,
            slow_subscriber: self.slow_subscriber,
        })
    }
}
