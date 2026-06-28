use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::Duration;

use rbus_core::warning::{Warning, WarningCode};

use crate::daemon_process::DaemonProcess;
use crate::topology::{ScenarioKind, Topology};
use crate::transport_kind::TransportKind;
use crate::wait_mode::WaitMode;
use crate::worker::{RESULT_PREFIX, WorkerRole};

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const READY_WAIT_ATTEMPTS: usize = 2_000;
const READY_WAIT_POLL_INTERVAL_MS: u64 = 1;
const STRACE_BINARY: &str = "strace";
const STRACE_CALLS_FIELD_INDEX: usize = 3;
const STRACE_MINIMUM_FIELD_COUNT: usize = 5;
const SYSCALL_DOMAIN_PREFIX: &str = "rbus-suite-syscalls";
const TRACE_FILE_EXTENSION: &str = "strace-summary";
const WORKER_TIMEOUT_MS: u64 = 60_000;

#[derive(Clone, Copy)]
pub(crate) struct SyscallMeasurementContext {
    pub(crate) run_index: usize,
    pub(crate) topology: Topology,
    pub(crate) transport: TransportKind,
    pub(crate) wait_mode: WaitMode,
    pub(crate) payload_size: usize,
    pub(crate) depth: usize,
    pub(crate) messages_per_publisher: u32,
}

pub(crate) fn measure_syscalls_per_message(
    context: SyscallMeasurementContext,
) -> Result<f64, Warning> {
    let domain = format!(
        "{SYSCALL_DOMAIN_PREFIX}-{}-{}",
        std::process::id(),
        context.run_index
    );
    let topic = format!("/bench/suite/syscalls/{}/data", context.run_index);
    let response_topic = format!("/bench/suite/syscalls/{}/response", context.run_index);
    let ready_directory = ready_directory(context.run_index);
    create_ready_directory(&ready_directory)?;
    let _daemon = if context.transport.is_rbus() {
        Some(DaemonProcess::start(&domain)?)
    } else {
        None
    };
    let run = SyscallRunContext {
        measurement: context,
        domain: &domain,
        topic: &topic,
        response_topic: &response_topic,
        ready_directory: &ready_directory,
    };
    let syscall_count = match context.topology.kind {
        ScenarioKind::OneWay | ScenarioKind::SlowSubscriber => run_one_way(run)?,
        ScenarioKind::PingPong => run_ping_pong(run)?,
        ScenarioKind::CrashSubscriber => run_crash_subscriber(run)?,
        ScenarioKind::CrashPublisher => run_crash_publisher(run)?,
    };
    remove_ready_directory(&ready_directory)?;
    Ok(syscall_count as f64 / measured_messages(context) as f64)
}

fn run_one_way(context: SyscallRunContext<'_>) -> Result<u64, Warning> {
    let mut subscribers = Vec::new();
    for subscriber_index in 0..context.measurement.topology.subscriber_count {
        let ready_file = ready_file(context.ready_directory, subscriber_index);
        subscribers.push(spawn_worker(
            worker_arguments(
                context,
                WorkerRole::Subscriber,
                subscriber_index,
                Some(&ready_file),
            ),
            trace_file(context, WorkerRole::Subscriber, subscriber_index),
        )?);
    }
    wait_ready_files(
        context.ready_directory,
        context.measurement.topology.subscriber_count,
    )?;
    let mut publishers = Vec::new();
    for publisher_index in 0..context.measurement.topology.publisher_count {
        publishers.push(spawn_worker(
            worker_arguments(context, WorkerRole::Publisher, publisher_index, None),
            trace_file(context, WorkerRole::Publisher, publisher_index),
        )?);
    }
    wait_for_workers(publishers, false)?
        .checked_add(wait_for_workers(subscribers, false)?)
        .ok_or_else(|| warning("run_one_way::overflow"))
}

fn run_ping_pong(context: SyscallRunContext<'_>) -> Result<u64, Warning> {
    let ready_file = ready_file(context.ready_directory, 0);
    let responder = spawn_worker(
        worker_arguments(context, WorkerRole::PingResponder, 0, Some(&ready_file)),
        trace_file(context, WorkerRole::PingResponder, 0),
    )?;
    wait_ready_files(context.ready_directory, 1)?;
    let requester = spawn_worker(
        worker_arguments(context, WorkerRole::PingRequester, 0, None),
        trace_file(context, WorkerRole::PingRequester, 0),
    )?;
    wait_for_worker(requester, false)?
        .checked_add(wait_for_worker(responder, false)?)
        .ok_or_else(|| warning("run_ping_pong::overflow"))
}

fn run_crash_subscriber(context: SyscallRunContext<'_>) -> Result<u64, Warning> {
    let ready_file = ready_file(context.ready_directory, 0);
    let crash_subscriber = spawn_worker(
        worker_arguments(context, WorkerRole::CrashSubscriber, 0, Some(&ready_file)),
        trace_file(context, WorkerRole::CrashSubscriber, 0),
    )?;
    wait_ready_files(context.ready_directory, 1)?;
    let publisher = spawn_worker(
        worker_arguments(context, WorkerRole::Publisher, 0, None),
        trace_file(context, WorkerRole::Publisher, 0),
    )?;
    wait_for_worker(publisher, false)?
        .checked_add(wait_for_worker(crash_subscriber, true)?)
        .ok_or_else(|| warning("run_crash_subscriber::overflow"))
}

fn run_crash_publisher(context: SyscallRunContext<'_>) -> Result<u64, Warning> {
    let ready_file = ready_file(context.ready_directory, 0);
    let subscriber = spawn_worker(
        worker_arguments(context, WorkerRole::Subscriber, 0, Some(&ready_file)),
        trace_file(context, WorkerRole::Subscriber, 0),
    )?;
    wait_ready_files(context.ready_directory, 1)?;
    let crash_publisher = spawn_worker(
        worker_arguments(context, WorkerRole::CrashPublisher, 0, None),
        trace_file(context, WorkerRole::CrashPublisher, 0),
    )?;
    wait_for_worker(crash_publisher, true)?
        .checked_add(wait_for_worker(subscriber, false)?)
        .ok_or_else(|| warning("run_crash_publisher::overflow"))
}

fn worker_arguments(
    context: SyscallRunContext<'_>,
    role: WorkerRole,
    worker_index: u8,
    ready_file: Option<&Path>,
) -> Vec<String> {
    let mut arguments = vec![
        String::from("--suite-worker"),
        String::from(role.as_str()),
        String::from("--domain"),
        context.domain.to_owned(),
        String::from("--topic"),
        context.topic.to_owned(),
        String::from("--response-topic"),
        context.response_topic.to_owned(),
        String::from("--payload-size"),
        context.measurement.payload_size.to_string(),
        String::from("--depth"),
        context.measurement.depth.to_string(),
        String::from("--worker-index"),
        worker_index.to_string(),
        String::from("--publisher-count"),
        context.measurement.topology.publisher_count.to_string(),
        String::from("--subscriber-count"),
        context.measurement.topology.subscriber_count.to_string(),
        String::from("--messages"),
        context.measurement.messages_per_publisher.to_string(),
        String::from("--wait-mode"),
        context.measurement.wait_mode.as_str().to_owned(),
        String::from("--transport"),
        context.measurement.transport.as_str().to_owned(),
    ];
    if context.measurement.topology.kind == ScenarioKind::SlowSubscriber
        && role == WorkerRole::Subscriber
    {
        arguments.push(String::from("--slow-subscriber"));
    }
    if let Some(path) = ready_file {
        arguments.push(String::from("--ready-file"));
        arguments.push(path.display().to_string());
    }
    arguments
}

fn spawn_worker(arguments: Vec<String>, trace_path: PathBuf) -> Result<TracedWorker, Warning> {
    let child = Command::new(STRACE_BINARY)
        .args(["--quiet=all", "--trace=all", "-c", "-o"])
        .arg(&trace_path)
        .arg("--")
        .arg(current_executable()?)
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| warning_from_io(error, "spawn_worker"))?;
    Ok(TracedWorker { child, trace_path })
}

fn wait_for_workers(workers: Vec<TracedWorker>, allow_failure: bool) -> Result<u64, Warning> {
    let mut syscall_count = 0_u64;
    for worker in workers {
        syscall_count = syscall_count
            .checked_add(wait_for_worker(worker, allow_failure)?)
            .ok_or_else(|| warning("wait_for_workers::overflow"))?;
    }
    Ok(syscall_count)
}

fn wait_for_worker(mut worker: TracedWorker, allow_failure: bool) -> Result<u64, Warning> {
    let started_at = crate::system_metrics::monotonic_time_ns()?;
    loop {
        if worker
            .child
            .try_wait()
            .map_err(|error| warning_from_io(error, "wait_for_worker::try_wait"))?
            .is_some()
        {
            break;
        }
        if crate::system_metrics::monotonic_time_ns()?.saturating_sub(started_at)
            > Duration::from_millis(WORKER_TIMEOUT_MS).as_nanos() as u64
        {
            worker
                .child
                .kill()
                .map_err(|error| warning_from_io(error, "wait_for_worker::kill"))?;
            return Err(warning("wait_for_worker::timeout"));
        }
        std::thread::sleep(Duration::from_millis(READY_WAIT_POLL_INTERVAL_MS));
    }
    let output = worker
        .child
        .wait_with_output()
        .map_err(|error| warning_from_io(error, "wait_for_worker::output"))?;
    validate_worker_output(&output, allow_failure)?;
    let syscall_count = parse_strace_summary(&worker.trace_path)?;
    std::fs::remove_file(&worker.trace_path)
        .map_err(|error| warning_from_io(error, "trace_file::remove"))?;
    Ok(syscall_count)
}

fn validate_worker_output(output: &Output, allow_failure: bool) -> Result<(), Warning> {
    if !output.stderr.is_empty() {
        print_child_stderr(output);
    }
    if !output.status.success() && !allow_failure {
        return Err(warning("wait_for_worker::status"));
    }
    if output.status.success() && allow_failure {
        return Err(warning("wait_for_worker::expected_crash"));
    }
    if allow_failure {
        return Ok(());
    }
    let stdout =
        String::from_utf8(output.stdout.clone()).map_err(|_| warning("worker_stdout::utf8"))?;
    let mut report_found = false;
    for line in stdout.lines() {
        if let Some(report) = line.strip_prefix(RESULT_PREFIX) {
            report_found = true;
            if worker_warning_count(report)? != 0 {
                return Err(warning("worker_report::warnings"));
            }
        }
    }
    if !report_found {
        return Err(warning("worker_report::missing"));
    }
    Ok(())
}

fn worker_warning_count(report: &str) -> Result<u64, Warning> {
    for field in report.split_whitespace() {
        if let Some(value) = field.strip_prefix("warnings=") {
            return value
                .parse::<u64>()
                .map_err(|_| warning("worker_report::warning_count"));
        }
    }
    Err(warning("worker_report::warning_count_missing"))
}

fn parse_strace_summary(path: &Path) -> Result<u64, Warning> {
    let contents = std::fs::read_to_string(path)
        .map_err(|error| warning_from_io(error, "parse_strace_summary::read"))?;
    let mut summed_calls = 0_u64;
    let mut total_calls = None;
    for line in contents.lines() {
        if let Some(row) = parse_strace_summary_row(line)? {
            if row.is_total {
                total_calls = Some(row.calls);
            } else {
                summed_calls = summed_calls
                    .checked_add(row.calls)
                    .ok_or_else(|| warning("parse_strace_summary::overflow"))?;
            }
        }
    }
    match total_calls {
        Some(calls) => Ok(calls),
        None if summed_calls != 0 => Ok(summed_calls),
        None => Err(warning("parse_strace_summary::empty")),
    }
}

fn parse_strace_summary_row(line: &str) -> Result<Option<StraceSummaryRow>, Warning> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < STRACE_MINIMUM_FIELD_COUNT || !is_percent_field(fields[0]) {
        return Ok(None);
    }
    let calls = fields[STRACE_CALLS_FIELD_INDEX]
        .parse::<u64>()
        .map_err(|_| warning("parse_strace_summary::calls"))?;
    Ok(Some(StraceSummaryRow {
        calls,
        is_total: fields.last() == Some(&"total"),
    }))
}

fn is_percent_field(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || byte == b'.')
}

fn measured_messages(context: SyscallMeasurementContext) -> u64 {
    match context.topology.kind {
        ScenarioKind::PingPong => u64::from(context.messages_per_publisher),
        ScenarioKind::CrashSubscriber | ScenarioKind::CrashPublisher => {
            u64::from(context.messages_per_publisher)
        }
        ScenarioKind::OneWay | ScenarioKind::SlowSubscriber => {
            u64::from(context.topology.publisher_count)
                * u64::from(context.topology.subscriber_count)
                * u64::from(context.messages_per_publisher)
        }
    }
}

fn wait_ready_files(directory: &Path, count: u8) -> Result<(), Warning> {
    for _attempt in 0..READY_WAIT_ATTEMPTS {
        let mut ready_count = 0_u8;
        for index in 0..count {
            if ready_file(directory, index).exists() {
                ready_count = ready_count.saturating_add(1);
            }
        }
        if ready_count == count {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(READY_WAIT_POLL_INTERVAL_MS));
    }
    Err(warning("wait_ready_files::timeout"))
}

fn ready_directory(run_index: usize) -> PathBuf {
    std::env::temp_dir().join(format!(
        "rbus-bench-syscalls-{}-{run_index}",
        std::process::id()
    ))
}

fn ready_file(directory: &Path, index: u8) -> PathBuf {
    directory.join(format!("subscriber-{index}.ready"))
}

fn trace_file(context: SyscallRunContext<'_>, role: WorkerRole, worker_index: u8) -> PathBuf {
    context.ready_directory.join(format!(
        "{}-{worker_index}.{TRACE_FILE_EXTENSION}",
        role.as_str()
    ))
}

fn create_ready_directory(directory: &Path) -> Result<(), Warning> {
    std::fs::create_dir_all(directory).map_err(|error| warning_from_io(error, "ready_dir::create"))
}

fn remove_ready_directory(directory: &Path) -> Result<(), Warning> {
    std::fs::remove_dir_all(directory).map_err(|error| warning_from_io(error, "ready_dir::remove"))
}

fn current_executable() -> Result<PathBuf, Warning> {
    std::env::current_exe().map_err(|error| warning_from_io(error, "current_executable"))
}

fn print_child_stderr(output: &Output) {
    if !output.stderr.is_empty() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    }
}

fn warning(operation: &'static str) -> Warning {
    Warning::new(WarningCode::InvalidConfig, COMPONENT_RBUS_BENCH, operation)
}

fn warning_from_io(error: std::io::Error, operation: &'static str) -> Warning {
    match error.raw_os_error() {
        Some(errno) => Warning::new(WarningCode::SyscallFailed, COMPONENT_RBUS_BENCH, operation)
            .with_os_errno(errno),
        None => Warning::new(WarningCode::SyscallFailed, COMPONENT_RBUS_BENCH, operation),
    }
}

struct TracedWorker {
    child: Child,
    trace_path: PathBuf,
}

#[derive(Clone, Copy)]
struct SyscallRunContext<'a> {
    measurement: SyscallMeasurementContext,
    domain: &'a str,
    topic: &'a str,
    response_topic: &'a str,
    ready_directory: &'a Path,
}

struct StraceSummaryRow {
    calls: u64,
    is_total: bool,
}
