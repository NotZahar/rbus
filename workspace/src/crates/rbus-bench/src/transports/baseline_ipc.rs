use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

use rbus_core::warning::{Warning, WarningCode};

use crate::allocation;
use crate::load_mode::LoadMode;
use crate::pace::{PaceSchedule, spin_tail_ns_from_us, wait_until};
use crate::system_metrics::{ProcessSystemSnapshot, monotonic_time_ns};
use crate::timestamp_payload::{PayloadTimestamps, decode_timestamps, encode_timestamps};
use crate::transport_kind::TransportKind;
use crate::transports::baseline_paths::{remove_path, stream_path};
use crate::worker::{
    END_TO_END_LATENCY_PREFIX, INTER_PUBLISH_GAP_PREFIX, INTER_RECEIVE_GAP_PREFIX,
    PUBLISH_LATENESS_PREFIX, ROUND_TRIP_PREFIX, TRANSPORT_LATENCY_PREFIX, WorkerArguments,
    WorkerReport, WorkerRole, warmup_messages_per_publisher,
};

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const BASELINE_READY_BYTES: &[u8] = b"ready\n";
const CONNECT_RETRY_ATTEMPTS: usize = 2_000;
const CONNECT_RETRY_INTERVAL_MS: u64 = 1;
const FILL_BYTE: u8 = 0xA5;
const PACED_DRAIN_TIMEOUT_MS: u64 = 5_000;
const PACING_PREWARM_SLEEP_NS: u64 = 1;
const SLOW_SUBSCRIBER_DELAY_MICROS: u64 = 100;
const START_WAIT_POLL_INTERVAL_MS: u64 = 1;
const START_WAIT_TIMEOUT_MS: u64 = 300_000;
const UNSET_TIMESTAMP_NS: u64 = 0;

pub fn run_worker(arguments: &WorkerArguments) -> Result<Option<WorkerReport>, Warning> {
    if arguments.transport.is_rbus() {
        return Ok(None);
    }
    let report = match arguments.role {
        WorkerRole::Publisher => run_publisher(arguments)?,
        WorkerRole::Subscriber => run_subscriber(arguments)?,
        WorkerRole::PingResponder => run_ping_responder(arguments)?,
        WorkerRole::PingRequester => run_ping_requester(arguments)?,
        WorkerRole::CrashSubscriber | WorkerRole::CrashPublisher => return Err(invalid_argument()),
    };
    Ok(Some(report))
}

fn run_publisher(arguments: &WorkerArguments) -> Result<WorkerReport, Warning> {
    let mut sender = BaselineSender::connect(arguments, &arguments.topic)?;
    let mut payload = vec![FILL_BYTE; arguments.payload_size];
    write_attach_ready_file(arguments)?;
    wait_warmup_start_file(arguments)?;
    for _message_index in 0..warmup_messages_per_publisher(arguments) {
        send_payload(arguments, &mut sender, &mut payload, true)?;
    }
    if arguments.load_mode == LoadMode::Paced {
        prewarm_pacing_driver();
    }
    write_ready_file(arguments)?;
    let start_ns = wait_start_file(arguments)?;
    match arguments.load_mode {
        LoadMode::Saturated => measure_report(u64::from(arguments.messages_per_publisher), || {
            for _message_index in 0..arguments.messages_per_publisher {
                send_payload(arguments, &mut sender, &mut payload, false)?;
            }
            Ok(())
        }),
        LoadMode::Paced => run_paced_publisher(arguments, &mut sender, &mut payload, start_ns),
    }
}

fn run_subscriber(arguments: &WorkerArguments) -> Result<WorkerReport, Warning> {
    let mut receiver = BaselineReceiver::bind(arguments, &arguments.topic)?;
    write_attach_ready_file(arguments)?;
    receiver.accept(arguments.publisher_count)?;
    wait_warmup_start_file(arguments)?;
    let mut payload = vec![0_u8; arguments.payload_size];
    let warmup_messages = u64::from(arguments.publisher_count)
        * u64::try_from(warmup_messages_per_publisher(arguments))
            .map_err(|_| invalid_argument())?;
    for _message_index in 0..warmup_messages {
        receiver.receive(&mut payload)?;
    }
    write_ready_file(arguments)?;
    let start_ns = wait_start_file(arguments)?;
    if arguments.load_mode == LoadMode::Paced {
        receiver.set_read_timeout(Duration::from_millis(PACED_DRAIN_TIMEOUT_MS))?;
    }
    let measured_messages =
        u64::from(arguments.publisher_count) * u64::from(arguments.messages_per_publisher);
    let mut transport_latencies = vec![0_u64; usize_from_u64(measured_messages)?];
    let mut end_to_end_latencies = vec![0_u64; usize_from_u64(measured_messages)?];
    let mut inter_receive_gaps = Vec::with_capacity(transport_latencies.len().saturating_sub(1));
    let drain_deadline_ns = paced_drain_deadline_ns(arguments, start_ns)?;
    let mut previous_receive_ns = None;
    let report = measure_report(measured_messages, || {
        for (transport_latency, end_to_end_latency) in transport_latencies
            .iter_mut()
            .zip(end_to_end_latencies.iter_mut())
        {
            match arguments.load_mode {
                LoadMode::Saturated => receiver.receive(&mut payload)?,
                LoadMode::Paced => receiver.receive_until(&mut payload, drain_deadline_ns)?,
            }
            let received_at = monotonic_time_ns()?;
            if let Some(previous_receive_ns) = previous_receive_ns {
                inter_receive_gaps.push(received_at.saturating_sub(previous_receive_ns));
            }
            previous_receive_ns = Some(received_at);
            let timestamps = decode_timestamps(&payload)?;
            *transport_latency = received_at.saturating_sub(timestamps.transport_ns);
            *end_to_end_latency = received_at.saturating_sub(timestamps.end_to_end_ns);
            if arguments.slow_subscriber {
                std::thread::sleep(Duration::from_micros(SLOW_SUBSCRIBER_DELAY_MICROS));
            }
        }
        Ok(())
    })?;
    print_one_way_latencies(transport_latencies, end_to_end_latencies);
    print_prefixed_samples(INTER_RECEIVE_GAP_PREFIX, inter_receive_gaps);
    Ok(report)
}

fn run_paced_publisher(
    arguments: &WorkerArguments,
    sender: &mut BaselineSender,
    payload: &mut [u8],
    start_ns: u64,
) -> Result<WorkerReport, Warning> {
    let rate_hz = arguments.rate_hz.ok_or_else(invalid_argument)?;
    let schedule = PaceSchedule::new(start_ns, rate_hz);
    let spin_tail_ns = spin_tail_ns_from_us(arguments.pace_spin_tail_us)?;
    let miss_threshold_ns = schedule.period_ns()?;
    let sample_capacity = arguments.messages_per_publisher as usize;
    let mut publish_lateness = Vec::with_capacity(sample_capacity);
    let mut inter_publish_gaps = Vec::with_capacity(sample_capacity.saturating_sub(1));
    let mut publish_deadline_miss_count = 0_u64;
    let mut previous_publish_ns = None;
    let mut report = measure_report(u64::from(arguments.messages_per_publisher), || {
        for message_index in 0..arguments.messages_per_publisher {
            let deadline_ns = schedule.deadline_ns(message_index)?;
            let lateness_ns = wait_until(deadline_ns, spin_tail_ns)?;
            if lateness_ns > miss_threshold_ns {
                publish_deadline_miss_count = publish_deadline_miss_count.saturating_add(1);
            }
            publish_lateness.push(lateness_ns);
            send_payload(arguments, sender, payload, false)?;
            let published_at_ns = monotonic_time_ns()?;
            if let Some(previous_publish_ns) = previous_publish_ns {
                inter_publish_gaps.push(published_at_ns.saturating_sub(previous_publish_ns));
            }
            previous_publish_ns = Some(published_at_ns);
        }
        Ok(())
    })?;
    report.publish_deadline_miss_count = publish_deadline_miss_count;
    print_prefixed_samples(PUBLISH_LATENESS_PREFIX, publish_lateness);
    print_prefixed_samples(INTER_PUBLISH_GAP_PREFIX, inter_publish_gaps);
    Ok(report)
}

fn run_ping_responder(arguments: &WorkerArguments) -> Result<WorkerReport, Warning> {
    let mut receiver = BaselineReceiver::bind(arguments, &arguments.topic)?;
    write_attach_ready_file(arguments)?;
    receiver.accept(1)?;
    wait_warmup_start_file(arguments)?;
    let mut sender =
        BaselineSender::connect_to_subscriber(arguments, &arguments.response_topic, 0)?;
    let mut payload = vec![FILL_BYTE; arguments.payload_size];
    for _message_index in 0..warmup_messages_per_publisher(arguments) {
        receiver.receive(&mut payload)?;
        let timestamp_ns = decode_timestamps(&payload)?.end_to_end_ns;
        send_payload_with_timestamp(arguments, &mut sender, &mut payload, timestamp_ns, true)?;
    }
    write_ready_file(arguments)?;
    wait_start_file(arguments)?;
    measure_report(u64::from(arguments.messages_per_publisher), || {
        for _message_index in 0..arguments.messages_per_publisher {
            receiver.receive(&mut payload)?;
            let timestamp_ns = decode_timestamps(&payload)?.end_to_end_ns;
            send_payload_with_timestamp(arguments, &mut sender, &mut payload, timestamp_ns, false)?;
        }
        Ok(())
    })
}

fn run_ping_requester(arguments: &WorkerArguments) -> Result<WorkerReport, Warning> {
    let mut receiver = BaselineReceiver::bind(arguments, &arguments.response_topic)?;
    let mut sender = BaselineSender::connect_to_subscriber(arguments, &arguments.topic, 0)?;
    receiver.accept(1)?;
    let mut payload = vec![FILL_BYTE; arguments.payload_size];
    write_attach_ready_file(arguments)?;
    wait_warmup_start_file(arguments)?;
    for _message_index in 0..warmup_messages_per_publisher(arguments) {
        send_payload_with_timestamp(
            arguments,
            &mut sender,
            &mut payload,
            monotonic_time_ns()?,
            true,
        )?;
        receiver.receive(&mut payload)?;
    }
    write_ready_file(arguments)?;
    wait_start_file(arguments)?;
    let mut latencies = vec![0_u64; arguments.messages_per_publisher as usize];
    let report = measure_report(u64::from(arguments.messages_per_publisher), || {
        for latency in &mut latencies {
            send_payload_with_timestamp(
                arguments,
                &mut sender,
                &mut payload,
                monotonic_time_ns()?,
                false,
            )?;
            receiver.receive(&mut payload)?;
            *latency =
                monotonic_time_ns()?.saturating_sub(decode_timestamps(&payload)?.end_to_end_ns);
        }
        Ok(())
    })?;
    for latency in latencies {
        println!("{ROUND_TRIP_PREFIX}{latency}");
    }
    Ok(report)
}

fn measure_report(
    messages: u64,
    operation: impl FnOnce() -> Result<(), Warning>,
) -> Result<WorkerReport, Warning> {
    let system_start = ProcessSystemSnapshot::capture()?;
    let allocation_start = allocation::allocation_count();
    let started_at = monotonic_time_ns()?;
    operation()?;
    let duration_ns = monotonic_time_ns()?.saturating_sub(started_at);
    let allocations = allocation::allocation_delta(allocation_start);
    let system_delta = ProcessSystemSnapshot::capture()?.delta(system_start);
    Ok(WorkerReport {
        messages,
        duration_ns,
        allocations,
        cpu_time_ns: system_delta.cpu_time_ns,
        context_switches: system_delta.context_switches,
        rss_bytes: system_delta.rss_bytes,
        pss_bytes: system_delta.pss_bytes,
        warnings: 0,
        skipped_messages: 0,
        backpressure_pinned_slot: 0,
        publish_deadline_miss_count: 0,
    })
}

struct BaselineReceiver {
    listener: Option<UnixListener>,
    streams: Vec<UnixStream>,
    next_stream_index: usize,
    path: PathBuf,
}

impl BaselineReceiver {
    fn bind(arguments: &WorkerArguments, topic: &str) -> Result<Self, Warning> {
        let path = stream_path(arguments, topic, "sock", arguments.worker_index);
        remove_path(&path)?;
        let listener = UnixListener::bind(&path).map_err(warning_from_io)?;
        Ok(Self {
            listener: Some(listener),
            streams: Vec::new(),
            next_stream_index: 0,
            path,
        })
    }

    fn accept(&mut self, expected_streams: u8) -> Result<(), Warning> {
        let listener = self.listener.take().ok_or_else(invalid_argument)?;
        for _stream_index in 0..expected_streams {
            let (stream, _address) = listener.accept().map_err(warning_from_io)?;
            self.streams.push(stream);
        }
        remove_path(&self.path)
    }

    fn receive(&mut self, payload: &mut [u8]) -> Result<(), Warning> {
        let stream_index = self.next_stream_index % self.streams.len();
        self.streams[stream_index]
            .read_exact(payload)
            .map_err(warning_from_io)?;
        self.next_stream_index = (self.next_stream_index + 1) % self.streams.len();
        Ok(())
    }

    fn receive_until(&mut self, payload: &mut [u8], deadline_ns: u64) -> Result<(), Warning> {
        if monotonic_time_ns()? > deadline_ns {
            return Err(Warning::new(
                WarningCode::Timeout,
                COMPONENT_RBUS_BENCH,
                "baseline_receive_drain",
            ));
        }
        self.receive(payload)
    }

    fn set_read_timeout(&self, timeout: Duration) -> Result<(), Warning> {
        for stream in &self.streams {
            stream
                .set_read_timeout(Some(timeout))
                .map_err(warning_from_io)?;
        }
        Ok(())
    }
}

impl Drop for BaselineReceiver {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

struct BaselineSender {
    streams: Vec<UnixStream>,
}

impl BaselineSender {
    fn connect(arguments: &WorkerArguments, topic: &str) -> Result<Self, Warning> {
        match arguments.transport {
            TransportKind::UnixSocket | TransportKind::UnixSocketEndToEnd => {
                let mut streams = Vec::new();
                for subscriber_index in 0..arguments.subscriber_count {
                    let path = stream_path(arguments, topic, "sock", subscriber_index);
                    streams.push(retry_connect(|| {
                        UnixStream::connect(&path).map_err(warning_from_io)
                    })?);
                }
                Ok(Self { streams })
            }
            _ => Err(invalid_argument()),
        }
    }

    fn connect_to_subscriber(
        arguments: &WorkerArguments,
        topic: &str,
        subscriber_index: u8,
    ) -> Result<Self, Warning> {
        let mut single_subscriber_arguments = arguments.clone();
        single_subscriber_arguments.subscriber_count = 1;
        single_subscriber_arguments.worker_index = subscriber_index;
        Self::connect(&single_subscriber_arguments, topic)
    }

    fn send(&mut self, payload: &[u8]) -> Result<(), Warning> {
        for stream in &mut self.streams {
            stream.write_all(payload).map_err(warning_from_io)?;
        }
        Ok(())
    }
}

fn send_payload(
    arguments: &WorkerArguments,
    sender: &mut BaselineSender,
    payload: &mut [u8],
    force_payload_fill: bool,
) -> Result<(), Warning> {
    let end_to_end_ns =
        if arguments.transport.fills_payload_in_measured_window() || force_payload_fill {
            monotonic_time_ns()?
        } else {
            UNSET_TIMESTAMP_NS
        };
    send_payload_with_timestamp(
        arguments,
        sender,
        payload,
        end_to_end_ns,
        force_payload_fill,
    )
}

fn send_payload_with_timestamp(
    arguments: &WorkerArguments,
    sender: &mut BaselineSender,
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
    )?;
    sender.send(payload)
}

fn retry_connect<Connection>(
    mut operation: impl FnMut() -> Result<Connection, Warning>,
) -> Result<Connection, Warning> {
    let mut last_warning = invalid_argument();
    for _attempt in 0..CONNECT_RETRY_ATTEMPTS {
        match operation() {
            Ok(connection) => return Ok(connection),
            Err(warning) => last_warning = warning,
        }
        std::thread::sleep(Duration::from_millis(CONNECT_RETRY_INTERVAL_MS));
    }
    Err(last_warning)
}

fn print_one_way_latencies(transport_latencies: Vec<u64>, end_to_end_latencies: Vec<u64>) {
    print_prefixed_samples(TRANSPORT_LATENCY_PREFIX, transport_latencies);
    print_prefixed_samples(END_TO_END_LATENCY_PREFIX, end_to_end_latencies);
}

fn print_prefixed_samples(prefix: &str, samples: Vec<u64>) {
    for sample in samples {
        println!("{prefix}{sample}");
    }
}

fn write_ready_file(arguments: &WorkerArguments) -> Result<(), Warning> {
    match &arguments.ready_file {
        Some(path) => std::fs::write(path, BASELINE_READY_BYTES).map_err(warning_from_io),
        None => Ok(()),
    }
}

fn write_attach_ready_file(arguments: &WorkerArguments) -> Result<(), Warning> {
    match &arguments.attach_ready_file {
        Some(path) => std::fs::write(path, BASELINE_READY_BYTES).map_err(warning_from_io),
        None => Ok(()),
    }
}

fn wait_start_file(arguments: &WorkerArguments) -> Result<u64, Warning> {
    let Some(path) = &arguments.start_file else {
        return monotonic_time_ns();
    };
    wait_flag_file(&arguments.start_file)?;
    let contents = std::fs::read_to_string(path).map_err(warning_from_io)?;
    let start_ns = contents
        .trim()
        .parse::<u64>()
        .map_err(|_| invalid_argument())?;
    let _lateness_ns = wait_until(start_ns, 0)?;
    Ok(start_ns)
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

fn usize_from_u64(value: u64) -> Result<usize, Warning> {
    usize::try_from(value).map_err(|_| invalid_argument())
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

fn prewarm_pacing_driver() {
    std::thread::sleep(Duration::from_nanos(PACING_PREWARM_SLEEP_NS));
}

fn warning_from_io(error: std::io::Error) -> Warning {
    match error.raw_os_error() {
        Some(errno) => Warning::new(
            WarningCode::SyscallFailed,
            COMPONENT_RBUS_BENCH,
            "baseline_ipc",
        )
        .with_os_errno(errno),
        None => Warning::new(
            WarningCode::SyscallFailed,
            COMPONENT_RBUS_BENCH,
            "baseline_ipc",
        ),
    }
}

fn invalid_argument() -> Warning {
    Warning::new(
        WarningCode::InvalidConfig,
        COMPONENT_RBUS_BENCH,
        "baseline_ipc",
    )
}
