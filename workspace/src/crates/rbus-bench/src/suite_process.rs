use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::thread::JoinHandle;
use std::time::Duration;

use rbus_core::warning::{Warning, WarningCode};

use crate::suite_ready::wait_paths_or_worker_exit;
use crate::suite_worker_output::ParsedWorkerOutput;
use crate::system_metrics::monotonic_time_ns;
use crate::worker::{
    BACKLOG_PREFIX, END_TO_END_LATENCY_PREFIX, INTER_PUBLISH_GAP_PREFIX, INTER_RECEIVE_GAP_PREFIX,
    PUBLISH_LATENESS_PREFIX, RESULT_PREFIX, ROUND_TRIP_PREFIX, TRANSPORT_LATENCY_PREFIX,
    WorkerReport,
};

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const WORKER_REPORT_FIELD_COUNT: usize = 12;
const WORKER_TIMEOUT_MS: u64 = 300_000;
const WORKER_WAIT_POLL_INTERVAL_MS: u64 = 1;

pub(crate) fn spawn_worker(arguments: Vec<String>) -> Result<Child, Warning> {
    Command::new(current_executable()?)
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| warning_from_io(error, "spawn_worker"))
}

pub(crate) fn wait_for_worker(mut child: Child, allow_failure: bool) -> Result<Output, Warning> {
    let stdout_reader = child.stdout.take().map(spawn_pipe_reader);
    let stderr_reader = child.stderr.take().map(spawn_pipe_reader);
    let started_at = monotonic_time_ns()?;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| warning_from_io(error, "wait_for_worker::try_wait"))?
        {
            break status;
        }
        if monotonic_time_ns()?.saturating_sub(started_at)
            > Duration::from_millis(WORKER_TIMEOUT_MS).as_nanos() as u64
        {
            child
                .kill()
                .map_err(|error| warning_from_io(error, "wait_for_worker::kill"))?;
            child
                .wait()
                .map_err(|error| warning_from_io(error, "wait_for_worker::wait_after_kill"))?;
            let _stdout = join_pipe_reader(stdout_reader, "wait_for_worker::stdout_after_kill")?;
            let _stderr = join_pipe_reader(stderr_reader, "wait_for_worker::stderr_after_kill")?;
            return Err(warning("wait_for_worker::timeout"));
        }
        std::thread::sleep(Duration::from_millis(WORKER_WAIT_POLL_INTERVAL_MS));
    };
    let stdout = join_pipe_reader(stdout_reader, "wait_for_worker::stdout")?;
    let stderr = join_pipe_reader(stderr_reader, "wait_for_worker::stderr")?;
    let output = Output {
        status,
        stdout,
        stderr,
    };
    if !output.status.success() && !allow_failure {
        print_child_stderr(&output);
        return Err(warning("wait_for_worker::status"));
    }
    if output.status.success() && allow_failure {
        return Err(warning("wait_for_worker::expected_crash"));
    }
    Ok(output)
}

pub(crate) fn parse_worker_output(output: Output) -> Result<ParsedWorkerOutput, Warning> {
    if !output.stderr.is_empty() {
        print_child_stderr(&output);
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| warning("parse_worker_output::stdout_utf8"))?;
    let mut parsed = ParsedWorkerOutput::default();
    for line in stdout.lines() {
        if let Some(value) = line.strip_prefix(TRANSPORT_LATENCY_PREFIX) {
            parsed.transport_latencies.push(parse_latency(value)?);
        } else if let Some(value) = line.strip_prefix(END_TO_END_LATENCY_PREFIX) {
            parsed.end_to_end_latencies.push(parse_latency(value)?);
        } else if let Some(value) = line.strip_prefix(ROUND_TRIP_PREFIX) {
            parsed.round_trip_latencies.push(parse_latency(value)?);
        } else if let Some(value) = line.strip_prefix(PUBLISH_LATENESS_PREFIX) {
            parsed.publish_lateness.push(parse_latency(value)?);
        } else if let Some(value) = line.strip_prefix(INTER_PUBLISH_GAP_PREFIX) {
            parsed.inter_publish_gaps.push(parse_latency(value)?);
        } else if let Some(value) = line.strip_prefix(INTER_RECEIVE_GAP_PREFIX) {
            parsed.inter_receive_gaps.push(parse_latency(value)?);
        } else if let Some(value) = line.strip_prefix(BACKLOG_PREFIX) {
            parsed.ring_backlog_messages.push(parse_latency(value)?);
        } else if let Some(value) = line.strip_prefix(RESULT_PREFIX) {
            parsed.report = Some(parse_worker_report(value)?);
        }
    }
    Ok(parsed)
}

pub(crate) fn wait_worker_ready_files(
    paths: &[PathBuf],
    workers: &mut Vec<Child>,
) -> Result<(), Warning> {
    if let Some(worker) = wait_paths_or_worker_exit(paths, workers)? {
        let _output = wait_for_worker(worker, false)?;
        return Err(warning("wait_ready_files::worker_exit"));
    }
    Ok(())
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

fn parse_worker_report(line: &str) -> Result<WorkerReport, Warning> {
    let mut report = WorkerReport::default();
    let mut field_count = 0_usize;
    for field in line.split_whitespace() {
        field_count = field_count.saturating_add(1);
        let (key, value) = field
            .split_once('=')
            .ok_or_else(|| warning("parse_worker_report::field"))?;
        match key {
            "role" => {}
            "messages" => report.messages = parse_report_value(value)?,
            "duration_ns" => report.duration_ns = parse_report_value(value)?,
            "allocations" => report.allocations = parse_report_value(value)?,
            "cpu_time_ns" => report.cpu_time_ns = parse_report_value(value)?,
            "context_switches" => report.context_switches = parse_report_value(value)?,
            "rss_bytes" => report.rss_bytes = parse_report_value(value)?,
            "pss_bytes" => report.pss_bytes = parse_report_value(value)?,
            "warnings" => report.warnings = parse_report_value(value)?,
            "skipped_messages" => report.skipped_messages = parse_report_value(value)?,
            "backpressure_pinned_slot" => {
                report.backpressure_pinned_slot = parse_report_value(value)?;
            }
            "publish_deadline_miss_count" => {
                report.publish_deadline_miss_count = parse_report_value(value)?;
            }
            _ => return Err(warning("parse_worker_report::key")),
        }
    }
    if field_count != WORKER_REPORT_FIELD_COUNT {
        return Err(warning("parse_worker_report::missing"));
    }
    Ok(report)
}

fn parse_latency(value: &str) -> Result<u64, Warning> {
    value.parse::<u64>().map_err(|_| warning("parse_latency"))
}

fn parse_report_value(value: &str) -> Result<u64, Warning> {
    value
        .parse::<u64>()
        .map_err(|_| warning("parse_worker_report::value"))
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
