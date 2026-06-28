use std::io::{Read, StdoutLock, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread::JoinHandle;
use std::time::Duration;

use rbus_core::warning::{Warning, WarningCode};

use crate::args::SuiteOptions;
use crate::baseline::enforce_regression_gates;
use crate::daemon_process::DaemonProcess;
use crate::metrics::{BenchmarkResult, BenchmarkResultContext};
use crate::output_format::OutputFormat;
use crate::report::{CSV_HEADER, SuiteReport, result_csv_line};
use crate::suite_matrix::{
    depth_for, measured_messages, messages_per_publisher_for_payload, selected_payload_sizes,
    selected_topologies, selected_wait_modes,
};
use crate::suite_ready::{
    attach_ready_file, create_ready_directory, ready_directory, remove_ready_directory, start_file,
    wait_paths_or_worker_exit, warmup_start_file, worker_ready_file, write_start_file,
    write_warmup_start_file,
};
use crate::suite_repetition::aggregate_repeated_results;
use crate::suite_worker_output::ParsedWorkerOutput;
use crate::syscall_measurement::{SyscallMeasurementContext, measure_syscalls_per_message};
use crate::system_metrics::monotonic_time_ns;
use crate::topology::{ScenarioKind, Topology};
use crate::transport_kind::{SUITE_TRANSPORTS, TransportKind};
use crate::wait_mode::WaitMode;
use crate::worker::{
    END_TO_END_LATENCY_PREFIX, RESULT_PREFIX, ROUND_TRIP_PREFIX, TRANSPORT_LATENCY_PREFIX,
    WorkerReport, WorkerRole,
};

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const SUITE_DOMAIN_PREFIX: &str = "rbus-suite";
const WORKER_WAIT_POLL_INTERVAL_MS: u64 = 1;
const WORKER_REPORT_FIELD_COUNT: usize = 11;
const WORKER_TIMEOUT_MS: u64 = 300_000;
const SUITE_TIMEOUT_MINUTES: u64 = 20;
const SECONDS_PER_MINUTE: u64 = 60;
const NANOS_PER_SECOND: u64 = 1_000_000_000;
const SUITE_TIMEOUT_NS: u64 = SUITE_TIMEOUT_MINUTES * SECONDS_PER_MINUTE * NANOS_PER_SECOND;

pub fn run_suite(options: &SuiteOptions) -> Result<SuiteReport, Warning> {
    run_suite_internal(options, None)
}

pub fn run_suite_csv_stream(options: &SuiteOptions) -> Result<(), Warning> {
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    writeln!(stdout, "{CSV_HEADER}").map_err(|error| warning_from_io(error, "csv::header"))?;
    stdout
        .flush()
        .map_err(|error| warning_from_io(error, "csv::flush_header"))?;
    let _report = run_suite_internal(options, Some(&mut stdout))?;
    Ok(())
}

fn run_suite_internal(
    options: &SuiteOptions,
    mut csv_stream: Option<&mut StdoutLock<'_>>,
) -> Result<SuiteReport, Warning> {
    let suite_started_at = monotonic_time_ns()?;
    let topologies = selected_topologies(options);
    let payload_sizes = selected_payload_sizes(options);
    let wait_modes = selected_wait_modes(options);
    let transports = SUITE_TRANSPORTS;
    let mut results = Vec::new();
    let mut run_index = 0_usize;

    for topology in topologies {
        for payload_size in payload_sizes {
            for wait_mode in wait_modes {
                for transport in transports {
                    if transport.supports_scenario(topology.kind) {
                        let mut repeated_results =
                            Vec::with_capacity(options.runs_per_scenario as usize);
                        let mut syscalls_per_message = None;
                        for _run_index in 0..options.runs_per_scenario {
                            ensure_suite_time_budget(suite_started_at)?;
                            let result = run_topology(
                                options,
                                *topology,
                                *payload_size,
                                *wait_mode,
                                *transport,
                                run_index,
                                syscalls_per_message,
                            )?;
                            syscalls_per_message = Some(result.syscalls_per_message);
                            repeated_results.push(result);
                            run_index = run_index.saturating_add(1);
                        }
                        let result = aggregate_repeated_results(&mut repeated_results);
                        if let Some(stream) = csv_stream.as_deref_mut() {
                            write_csv_result(stream, &result)?;
                        }
                        results.push(result);
                    }
                }
            }
        }
    }

    let gate_status = if options.gate_enabled {
        enforce_regression_gates(&options.baseline_path, &results)?;
        "ok"
    } else {
        "disabled"
    };
    Ok(SuiteReport::new(
        "ok",
        gate_status,
        topologies.len(),
        payload_sizes.len(),
        wait_modes.len(),
        transports.len(),
        results,
    ))
}

fn write_csv_result(stream: &mut StdoutLock<'_>, result: &BenchmarkResult) -> Result<(), Warning> {
    writeln!(stream, "{}", result_csv_line("ok", result))
        .map_err(|error| warning_from_io(error, "write_csv_result::line"))?;
    stream
        .flush()
        .map_err(|error| warning_from_io(error, "write_csv_result::flush"))
}

fn ensure_suite_time_budget(suite_started_at: u64) -> Result<(), Warning> {
    if monotonic_time_ns()?.saturating_sub(suite_started_at) > SUITE_TIMEOUT_NS {
        return Err(Warning::new(
            WarningCode::Timeout,
            COMPONENT_RBUS_BENCH,
            "run_suite::timeout",
        ));
    }
    Ok(())
}

pub fn print_suite_report(report: &SuiteReport, output_format: OutputFormat) {
    match output_format {
        OutputFormat::Json => println!("{}", report.to_json()),
        OutputFormat::Csv => print!("{}", report.to_csv()),
    }
}

fn run_topology(
    options: &SuiteOptions,
    topology: Topology,
    payload_size: usize,
    wait_mode: WaitMode,
    transport: TransportKind,
    run_index: usize,
    syscalls_per_message: Option<f64>,
) -> Result<BenchmarkResult, Warning> {
    let domain = format!("{SUITE_DOMAIN_PREFIX}-{}-{run_index}", std::process::id());
    let topic = format!("/bench/suite/{run_index}/data");
    let response_topic = format!("/bench/suite/{run_index}/response");
    let ready_directory = ready_directory(run_index);
    create_ready_directory(&ready_directory)?;
    let startup_started_at = monotonic_time_ns()?;
    let _daemon = if transport.is_rbus() {
        Some(DaemonProcess::start(&domain)?)
    } else {
        None
    };
    let startup_latency_ns = monotonic_time_ns()?.saturating_sub(startup_started_at);
    let messages_per_publisher = messages_per_publisher_for_payload(options, payload_size);
    let depth = depth_for(payload_size, messages_per_publisher);
    let scenario_context = ScenarioContext {
        topology,
        transport,
        wait_mode,
        payload_size,
        depth,
        messages_per_publisher,
        domain: &domain,
        topic: &topic,
        response_topic: &response_topic,
        ready_directory: &ready_directory,
        startup_latency_ns,
        run_index,
        syscalls_per_message,
    };
    let result = match topology.kind {
        ScenarioKind::OneWay | ScenarioKind::SlowSubscriber => run_one_way(scenario_context)?,
    };
    remove_ready_directory(&ready_directory)?;
    Ok(result)
}

fn run_one_way(context: ScenarioContext<'_>) -> Result<BenchmarkResult, Warning> {
    let mut subscribers = Vec::new();
    let mut attach_ready_files = Vec::new();
    let mut worker_ready_files = Vec::new();
    let attach_started_at = monotonic_time_ns()?;
    for subscriber_index in 0..context.topology.subscriber_count {
        let attach_ready_file = attach_ready_file(context.ready_directory, subscriber_index);
        let ready_file = worker_ready_file(
            context.ready_directory,
            WorkerRole::Subscriber.as_str(),
            subscriber_index,
        );
        attach_ready_files.push(attach_ready_file.clone());
        worker_ready_files.push(ready_file.clone());
        subscribers.push(spawn_worker(worker_arguments(
            context,
            WorkerRole::Subscriber,
            subscriber_index,
            Some(&ready_file),
            Some(&attach_ready_file),
        ))?);
    }
    wait_worker_ready_files(&attach_ready_files, &mut subscribers)?;
    let attach_latency_ns = monotonic_time_ns()?.saturating_sub(attach_started_at);
    let wall_started_at = monotonic_time_ns()?;
    let mut publishers = Vec::new();
    let mut publisher_attach_ready_files = Vec::new();
    for publisher_index in 0..context.topology.publisher_count {
        let attach_ready_file =
            worker_ready_file(context.ready_directory, "attach-publisher", publisher_index);
        let ready_file = worker_ready_file(
            context.ready_directory,
            WorkerRole::Publisher.as_str(),
            publisher_index,
        );
        publisher_attach_ready_files.push(attach_ready_file.clone());
        worker_ready_files.push(ready_file.clone());
        publishers.push(spawn_worker(worker_arguments(
            context,
            WorkerRole::Publisher,
            publisher_index,
            Some(&ready_file),
            Some(&attach_ready_file),
        ))?);
    }
    wait_worker_ready_files(&publisher_attach_ready_files, &mut publishers)?;
    write_warmup_start_file(context.ready_directory)?;
    let mut workers = Vec::new();
    workers.append(&mut publishers);
    workers.append(&mut subscribers);
    wait_worker_ready_files(&worker_ready_files, &mut workers)?;
    write_start_file(context.ready_directory)?;
    let mut parsed_outputs = Vec::new();
    for worker in workers {
        parsed_outputs.push(parse_worker_output(wait_for_worker(worker, false)?)?);
    }
    let wall_time_ns = monotonic_time_ns()?.saturating_sub(wall_started_at);
    aggregate_result(
        context,
        parsed_outputs,
        Vec::new(),
        wall_time_ns,
        attach_latency_ns,
    )
}

fn aggregate_result(
    context: ScenarioContext<'_>,
    outputs: Vec<ParsedWorkerOutput>,
    round_trip_samples: Vec<u64>,
    wall_time_ns: u64,
    attach_latency_ns: u64,
) -> Result<BenchmarkResult, Warning> {
    let mut transport_latency_samples = Vec::new();
    let mut end_to_end_latency_samples = Vec::new();
    let mut round_trip_samples = round_trip_samples;
    let mut reports = Vec::new();
    for output in outputs {
        transport_latency_samples.extend(output.transport_latencies);
        end_to_end_latency_samples.extend(output.end_to_end_latencies);
        round_trip_samples.extend(output.round_trip_latencies);
        reports.push(
            output
                .report
                .ok_or_else(|| warning("aggregate_result::missing_report"))?,
        );
    }
    let aggregate = aggregate_worker_reports(&reports);
    if aggregate.warnings != 0 {
        return Err(warning("aggregate_result::warnings"));
    }
    let syscalls_per_message = match context.syscalls_per_message {
        Some(value) => value,
        None => measure_syscalls_per_message(SyscallMeasurementContext {
            run_index: context.run_index,
            topology: context.topology,
            transport: context.transport,
            wait_mode: context.wait_mode,
            payload_size: context.payload_size,
            depth: context.depth,
            messages_per_publisher: context.messages_per_publisher,
        })?,
    };
    Ok(BenchmarkResult::from_samples(
        BenchmarkResultContext {
            scenario: context.topology.name,
            transport: context.transport,
            wait_mode: context.wait_mode,
            payload_size: context.payload_size,
            publisher_count: context.topology.publisher_count,
            subscriber_count: context.topology.subscriber_count,
            measured_messages: measured_messages(
                context.topology.kind,
                context.topology.publisher_count,
                context.topology.subscriber_count,
                context.messages_per_publisher,
            ),
            measured_window_ns: aggregate.duration_ns,
            orchestration_wall_time_ns: wall_time_ns,
            allocation_count: aggregate.allocations,
            cpu_time_ns: aggregate.cpu_time_ns,
            rss_bytes: aggregate.rss_bytes,
            pss_bytes: aggregate.pss_bytes,
            context_switches: aggregate.context_switches,
            syscalls_per_message,
            warning_count: aggregate.warnings,
            skipped_messages: aggregate.skipped_messages,
            backpressure_pinned_slot: aggregate.backpressure_pinned_slot,
            startup_latency_ns: context.startup_latency_ns,
            attach_latency_ns,
        },
        &mut transport_latency_samples,
        &mut end_to_end_latency_samples,
        &mut round_trip_samples,
    ))
}

fn worker_arguments(
    context: ScenarioContext<'_>,
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
        context.payload_size.to_string(),
        String::from("--depth"),
        context.depth.to_string(),
        String::from("--worker-index"),
        worker_index.to_string(),
        String::from("--publisher-count"),
        context.topology.publisher_count.to_string(),
        String::from("--subscriber-count"),
        context.topology.subscriber_count.to_string(),
        String::from("--messages"),
        context.messages_per_publisher.to_string(),
        String::from("--wait-mode"),
        context.wait_mode.as_str().to_owned(),
        String::from("--transport"),
        context.transport.as_str().to_owned(),
    ];
    if context.topology.kind == ScenarioKind::SlowSubscriber && role == WorkerRole::Subscriber {
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

fn spawn_worker(arguments: Vec<String>) -> Result<Child, Warning> {
    Command::new(current_executable()?)
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| warning_from_io(error, "spawn_worker"))
}

fn wait_for_worker(mut child: Child, allow_failure: bool) -> Result<Output, Warning> {
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

fn parse_worker_output(output: Output) -> Result<ParsedWorkerOutput, Warning> {
    if !output.stderr.is_empty() {
        print_child_stderr(&output);
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| warning("parse_worker_output::stdout_utf8"))?;
    let mut parsed = ParsedWorkerOutput {
        report: None,
        transport_latencies: Vec::new(),
        end_to_end_latencies: Vec::new(),
        round_trip_latencies: Vec::new(),
    };
    for line in stdout.lines() {
        if let Some(value) = line.strip_prefix(TRANSPORT_LATENCY_PREFIX) {
            parsed
                .transport_latencies
                .push(value.parse::<u64>().map_err(|_| warning("parse_latency"))?);
        } else if let Some(value) = line.strip_prefix(END_TO_END_LATENCY_PREFIX) {
            parsed
                .end_to_end_latencies
                .push(value.parse::<u64>().map_err(|_| warning("parse_latency"))?);
        } else if let Some(value) = line.strip_prefix(ROUND_TRIP_PREFIX) {
            parsed
                .round_trip_latencies
                .push(value.parse::<u64>().map_err(|_| warning("parse_latency"))?);
        } else if let Some(value) = line.strip_prefix(RESULT_PREFIX) {
            parsed.report = Some(parse_worker_report(value)?);
        }
    }
    Ok(parsed)
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
            _ => return Err(warning("parse_worker_report::key")),
        }
    }
    if field_count != WORKER_REPORT_FIELD_COUNT {
        return Err(warning("parse_worker_report::missing"));
    }
    Ok(report)
}

fn parse_report_value(value: &str) -> Result<u64, Warning> {
    value
        .parse::<u64>()
        .map_err(|_| warning("parse_worker_report::value"))
}

fn wait_worker_ready_files(paths: &[PathBuf], workers: &mut Vec<Child>) -> Result<(), Warning> {
    if let Some(worker) = wait_paths_or_worker_exit(paths, workers)? {
        let _output = wait_for_worker(worker, false)?;
        return Err(warning("wait_ready_files::worker_exit"));
    }
    Ok(())
}

fn aggregate_worker_reports(reports: &[WorkerReport]) -> WorkerReport {
    let mut aggregate = WorkerReport::default();
    for report in reports {
        aggregate.messages = aggregate.messages.saturating_add(report.messages);
        aggregate.duration_ns = aggregate.duration_ns.max(report.duration_ns);
        aggregate.allocations = aggregate.allocations.saturating_add(report.allocations);
        aggregate.cpu_time_ns = aggregate.cpu_time_ns.saturating_add(report.cpu_time_ns);
        aggregate.context_switches = aggregate
            .context_switches
            .saturating_add(report.context_switches);
        aggregate.rss_bytes = aggregate.rss_bytes.saturating_add(report.rss_bytes);
        aggregate.pss_bytes = aggregate.pss_bytes.saturating_add(report.pss_bytes);
        aggregate.warnings = aggregate.warnings.saturating_add(report.warnings);
        aggregate.skipped_messages = aggregate
            .skipped_messages
            .saturating_add(report.skipped_messages);
        aggregate.backpressure_pinned_slot = aggregate
            .backpressure_pinned_slot
            .saturating_add(report.backpressure_pinned_slot);
    }
    aggregate
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

#[derive(Clone, Copy)]
struct ScenarioContext<'a> {
    topology: Topology,
    transport: TransportKind,
    wait_mode: WaitMode,
    payload_size: usize,
    depth: usize,
    messages_per_publisher: u32,
    domain: &'a str,
    topic: &'a str,
    response_topic: &'a str,
    ready_directory: &'a Path,
    startup_latency_ns: u64,
    run_index: usize,
    syscalls_per_message: Option<f64>,
}
