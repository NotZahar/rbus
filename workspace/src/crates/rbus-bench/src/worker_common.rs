use std::time::Duration;

use rbus_core::warning::{Warning, WarningCode};

use crate::load_mode::LoadMode;
use crate::pace::wait_until;
use crate::system_metrics::monotonic_time_ns;
use crate::timestamp_payload::TIMESTAMP_BYTES;
use crate::worker::{
    END_TO_END_LATENCY_PREFIX, RESULT_PREFIX, TRANSPORT_LATENCY_PREFIX, WorkerArguments,
    WorkerReport, WorkerRole,
};

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const READY_FILE_BYTES: &[u8] = b"ready\n";
const START_WAIT_POLL_INTERVAL_MS: u64 = 1;
const START_WAIT_TIMEOUT_MS: u64 = 300_000;

pub(crate) fn write_ready_file(arguments: &WorkerArguments) -> Result<(), Warning> {
    match &arguments.ready_file {
        Some(path) => std::fs::write(path, READY_FILE_BYTES)
            .map_err(|error| warning_from_io(error, "write_ready_file")),
        None => Ok(()),
    }
}

pub(crate) fn write_attach_ready_file(arguments: &WorkerArguments) -> Result<(), Warning> {
    match &arguments.attach_ready_file {
        Some(path) => std::fs::write(path, READY_FILE_BYTES)
            .map_err(|error| warning_from_io(error, "write_attach_ready_file")),
        None => Ok(()),
    }
}

pub(crate) fn wait_start_file(arguments: &WorkerArguments) -> Result<u64, Warning> {
    let Some(path) = &arguments.start_file else {
        return monotonic_time_ns();
    };
    wait_flag_file(&arguments.start_file)?;
    let contents =
        std::fs::read_to_string(path).map_err(|error| warning_from_io(error, "read_start_file"))?;
    let start_ns = contents
        .trim()
        .parse::<u64>()
        .map_err(|_| invalid_argument())?;
    let _lateness_ns = wait_until(start_ns, 0)?;
    Ok(start_ns)
}

pub(crate) fn wait_warmup_start_file(arguments: &WorkerArguments) -> Result<(), Warning> {
    wait_flag_file(&arguments.warmup_start_file)
}

pub(crate) fn print_one_way_latencies(
    transport_latencies: Vec<u64>,
    end_to_end_latencies: Vec<u64>,
) {
    print_prefixed_samples(TRANSPORT_LATENCY_PREFIX, transport_latencies);
    print_prefixed_samples(END_TO_END_LATENCY_PREFIX, end_to_end_latencies);
}

pub(crate) fn print_prefixed_samples(prefix: &str, samples: Vec<u64>) {
    for sample in samples {
        println!("{prefix}{sample}");
    }
}

pub(crate) fn print_report(role: WorkerRole, report: WorkerReport) {
    println!(
        concat!(
            "{}role={} messages={} duration_ns={} allocations={} ",
            "cpu_time_ns={} context_switches={} rss_bytes={} pss_bytes={} ",
            "warnings={} skipped_messages={} backpressure_pinned_slot={} ",
            "publish_deadline_miss_count={}"
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
        report.backpressure_pinned_slot,
        report.publish_deadline_miss_count
    );
}

pub(crate) fn validate_worker_arguments(arguments: &WorkerArguments) -> Result<(), Warning> {
    if arguments.domain.is_empty()
        || arguments.topic.is_empty()
        || arguments.payload_size < TIMESTAMP_BYTES
        || arguments.depth == 0
        || !arguments.depth.is_power_of_two()
        || arguments.worker_index >= arguments.publisher_count.max(arguments.subscriber_count)
        || arguments.publisher_count == 0
        || arguments.subscriber_count == 0
        || arguments.messages_per_publisher == 0
        || (arguments.load_mode == LoadMode::Paced && arguments.rate_hz.is_none())
    {
        return Err(invalid_argument());
    }
    Ok(())
}

pub(crate) fn usize_from_u64(value: u64) -> Result<usize, Warning> {
    usize::try_from(value).map_err(|_| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_RBUS_BENCH,
            "usize_from_u64",
        )
    })
}

pub(crate) fn invalid_argument() -> Warning {
    Warning::new(
        WarningCode::InvalidConfig,
        COMPONENT_RBUS_BENCH,
        "WorkerArguments::parse",
    )
}

pub(crate) fn warning_from_io(error: std::io::Error, operation: &'static str) -> Warning {
    match error.raw_os_error() {
        Some(errno) => Warning::new(WarningCode::SyscallFailed, COMPONENT_RBUS_BENCH, operation)
            .with_os_errno(errno),
        None => Warning::new(WarningCode::SyscallFailed, COMPONENT_RBUS_BENCH, operation),
    }
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
