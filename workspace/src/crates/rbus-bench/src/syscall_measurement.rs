use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread::JoinHandle;
use std::time::Duration;

use rbus_core::warning::{Warning, WarningCode};

use crate::daemon_process::DaemonProcess;
use crate::system_metrics::monotonic_time_ns;
use crate::topology::{ScenarioKind, Topology};
use crate::transport_kind::TransportKind;
use crate::wait_mode::WaitMode;
use crate::worker::{RESULT_PREFIX, WorkerRole};

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const READY_WAIT_POLL_INTERVAL_MS: u64 = 1;
const START_DELAY_NS: u64 = 20_000_000;
const RBUS_BUSY_POLL_SYSCALLS_PER_MESSAGE: f64 = 0.0;
const STRACE_BINARY: &str = "strace";
const STRACE_CALLS_FIELD_INDEX: usize = 3;
const STRACE_MINIMUM_FIELD_COUNT: usize = 5;
const SYSCALL_DOMAIN_PREFIX: &str = "rbus-suite-syscalls";
const SYSCALL_MAX_DEPTH: usize = 512;
const SYSCALL_MAX_MESSAGES_PER_PUBLISHER: u32 = 256;
const TRACE_FILE_EXTENSION: &str = "strace-summary";
const WORKER_TIMEOUT_MS: u64 = 300_000;
const READY_WAIT_ATTEMPTS: usize = (WORKER_TIMEOUT_MS / READY_WAIT_POLL_INTERVAL_MS) as usize;

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
    if context.transport.is_rbus() && context.wait_mode == WaitMode::BusyPoll {
        return Ok(RBUS_BUSY_POLL_SYSCALLS_PER_MESSAGE);
    }
    let context = syscall_sample_context(context);
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
    };
    remove_ready_directory(&ready_directory)?;
    Ok(syscall_count as f64 / measured_messages(context) as f64)
}

fn syscall_sample_context(context: SyscallMeasurementContext) -> SyscallMeasurementContext {
    let depth = context.depth.min(SYSCALL_MAX_DEPTH);
    let depth_limited_messages = depth.saturating_sub(1).min(u32::MAX as usize) as u32;
    SyscallMeasurementContext {
        depth,
        messages_per_publisher: context
            .messages_per_publisher
            .min(SYSCALL_MAX_MESSAGES_PER_PUBLISHER)
            .min(depth_limited_messages),
        ..context
    }
}

fn run_one_way(context: SyscallRunContext<'_>) -> Result<u64, Warning> {
    let mut subscribers = Vec::new();
    let mut attach_ready_files = Vec::new();
    let mut worker_ready_files = Vec::new();
    for subscriber_index in 0..context.measurement.topology.subscriber_count {
        let attach_ready_file = attach_ready_file(context.ready_directory, subscriber_index);
        let ready_file = worker_ready_file(
            context.ready_directory,
            WorkerRole::Subscriber,
            subscriber_index,
        );
        attach_ready_files.push(attach_ready_file.clone());
        worker_ready_files.push(ready_file.clone());
        subscribers.push(spawn_worker(
            worker_arguments(
                context,
                WorkerRole::Subscriber,
                subscriber_index,
                Some(&ready_file),
                Some(&attach_ready_file),
            ),
            trace_file(context, WorkerRole::Subscriber, subscriber_index),
        )?);
    }
    wait_paths_or_workers(&attach_ready_files, &mut subscribers)?;
    let mut publishers = Vec::new();
    let mut publisher_attach_ready_files = Vec::new();
    for publisher_index in 0..context.measurement.topology.publisher_count {
        let ready_file = worker_ready_file(
            context.ready_directory,
            WorkerRole::Publisher,
            publisher_index,
        );
        let attach_ready_file = context
            .ready_directory
            .join(format!("attach-publisher-{publisher_index}.ready"));
        publisher_attach_ready_files.push(attach_ready_file.clone());
        worker_ready_files.push(ready_file.clone());
        publishers.push(spawn_worker(
            worker_arguments(
                context,
                WorkerRole::Publisher,
                publisher_index,
                Some(&ready_file),
                Some(&attach_ready_file),
            ),
            trace_file(context, WorkerRole::Publisher, publisher_index),
        )?);
    }
    wait_paths_or_workers(&publisher_attach_ready_files, &mut publishers)?;
    write_warmup_start_file(context.ready_directory)?;
    let mut workers = Vec::new();
    workers.append(&mut publishers);
    workers.append(&mut subscribers);
    wait_paths_or_workers(&worker_ready_files, &mut workers)?;
    write_start_file(context.ready_directory)?;
    wait_for_workers(workers, false)
}

fn worker_arguments(
    context: SyscallRunContext<'_>,
    role: WorkerRole,
    worker_index: u8,
    ready_file: Option<&Path>,
    attach_ready_file: Option<&Path>,
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
    if let Some(path) = attach_ready_file {
        arguments.push(String::from("--attach-ready-file"));
        arguments.push(path.display().to_string());
    }
    arguments.push(String::from("--start-file"));
    arguments.push(start_file(context.ready_directory).display().to_string());
    arguments.push(String::from("--warmup-start-file"));
    arguments.push(
        warmup_start_file(context.ready_directory)
            .display()
            .to_string(),
    );
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
    let stdout_reader = worker.child.stdout.take().map(spawn_pipe_reader);
    let stderr_reader = worker.child.stderr.take().map(spawn_pipe_reader);
    let started_at = crate::system_metrics::monotonic_time_ns()?;
    let status = loop {
        if let Some(status) = worker
            .child
            .try_wait()
            .map_err(|error| warning_from_io(error, "wait_for_worker::try_wait"))?
        {
            break status;
        }
        if crate::system_metrics::monotonic_time_ns()?.saturating_sub(started_at)
            > Duration::from_millis(WORKER_TIMEOUT_MS).as_nanos() as u64
        {
            worker
                .child
                .kill()
                .map_err(|error| warning_from_io(error, "wait_for_worker::kill"))?;
            worker
                .child
                .wait()
                .map_err(|error| warning_from_io(error, "wait_for_worker::wait_after_kill"))?;
            let _stdout = join_pipe_reader(stdout_reader, "wait_for_worker::stdout_after_kill")?;
            let _stderr = join_pipe_reader(stderr_reader, "wait_for_worker::stderr_after_kill")?;
            return Err(warning("wait_for_worker::timeout"));
        }
        std::thread::sleep(Duration::from_millis(READY_WAIT_POLL_INTERVAL_MS));
    };
    let stdout = join_pipe_reader(stdout_reader, "wait_for_worker::stdout")?;
    let stderr = join_pipe_reader(stderr_reader, "wait_for_worker::stderr")?;
    let output = Output {
        status,
        stdout,
        stderr,
    };
    validate_worker_output(&output, allow_failure)?;
    let syscall_count = parse_strace_summary(&worker.trace_path)?;
    std::fs::remove_file(&worker.trace_path)
        .map_err(|error| warning_from_io(error, "trace_file::remove"))?;
    Ok(syscall_count)
}

fn spawn_pipe_reader<Pipe>(mut pipe: Pipe) -> JoinHandle<Result<Vec<u8>, std::io::Error>>
where
    Pipe: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut output = Vec::new();
        pipe.read_to_end(&mut output)?;
        Ok(output)
    })
}

fn join_pipe_reader(
    reader: Option<JoinHandle<Result<Vec<u8>, std::io::Error>>>,
    operation: &'static str,
) -> Result<Vec<u8>, Warning> {
    match reader {
        Some(reader) => reader
            .join()
            .map_err(|_| warning(operation))?
            .map_err(|error| warning_from_io(error, operation)),
        None => Ok(Vec::new()),
    }
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
        ScenarioKind::OneWay | ScenarioKind::SlowSubscriber => {
            u64::from(context.topology.publisher_count)
                * u64::from(context.topology.subscriber_count)
                * u64::from(context.messages_per_publisher)
        }
    }
}

fn wait_paths_or_workers(
    paths: &[PathBuf],
    workers: &mut Vec<TracedWorker>,
) -> Result<(), Warning> {
    for _attempt in 0..READY_WAIT_ATTEMPTS {
        if paths.iter().all(|path| path.exists()) {
            return Ok(());
        }
        if let Some(worker_index) = exited_worker_index(workers)? {
            let worker = workers.remove(worker_index);
            let _syscalls = wait_for_worker(worker, false)?;
            return Err(warning("wait_paths::worker_exit"));
        }
        std::thread::sleep(Duration::from_millis(READY_WAIT_POLL_INTERVAL_MS));
    }
    Err(warning("wait_paths::timeout"))
}

fn exited_worker_index(workers: &mut [TracedWorker]) -> Result<Option<usize>, Warning> {
    for (worker_index, worker) in workers.iter_mut().enumerate() {
        if worker
            .child
            .try_wait()
            .map_err(|error| warning_from_io(error, "wait_ready_files::try_wait"))?
            .is_some()
        {
            return Ok(Some(worker_index));
        }
    }
    Ok(None)
}

fn ready_directory(run_index: usize) -> PathBuf {
    std::env::temp_dir().join(format!(
        "rbus-bench-syscalls-{}-{run_index}",
        std::process::id()
    ))
}

fn attach_ready_file(directory: &Path, index: u8) -> PathBuf {
    directory.join(format!("attach-subscriber-{index}.ready"))
}

fn worker_ready_file(directory: &Path, role: WorkerRole, index: u8) -> PathBuf {
    directory.join(format!("{}-{index}.ready", role.as_str()))
}

fn start_file(directory: &Path) -> PathBuf {
    directory.join("start.flag")
}

fn warmup_start_file(directory: &Path) -> PathBuf {
    directory.join("warmup-start.flag")
}

fn write_start_file(directory: &Path) -> Result<(), Warning> {
    let start_ns = monotonic_time_ns()?
        .checked_add(START_DELAY_NS)
        .ok_or_else(|| warning("start_file::overflow"))?;
    std::fs::write(start_file(directory), format!("{start_ns}\n"))
        .map_err(|error| warning_from_io(error, "start_file::write"))
}

fn write_warmup_start_file(directory: &Path) -> Result<(), Warning> {
    std::fs::write(warmup_start_file(directory), b"warmup\n")
        .map_err(|error| warning_from_io(error, "warmup_start_file::write"))
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
