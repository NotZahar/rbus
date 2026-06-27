use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::Duration;

use rbus_core::warning::{Warning, WarningCode};

use crate::args::SuiteOptions;
use crate::baseline::enforce_regression_gates;
use crate::daemon_process::DaemonProcess;
use crate::metrics::{BenchmarkResult, BenchmarkResultContext};
use crate::output_format::OutputFormat;
use crate::payload::PAYLOAD_SIZES;
use crate::report::SuiteReport;
use crate::system_metrics::monotonic_time_ns;
use crate::topology::{ScenarioKind, TOPOLOGIES, Topology};
use crate::transport_kind::{SUITE_TRANSPORTS, TransportKind};
use crate::wait_mode::{WAIT_MODES, WaitMode};
use crate::worker::{
    LATENCY_PREFIX, RESULT_PREFIX, WorkerReport, WorkerRole, warmup_messages_per_publisher,
};

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const DEPTH_MINIMUM: usize = 8;
const READY_WAIT_ATTEMPTS: usize = 2_000;
const READY_WAIT_POLL_INTERVAL_MS: u64 = 1;
const SUITE_DOMAIN_PREFIX: &str = "rbus-suite";
const SYS_CALLS_PER_BLOCKING_MESSAGE: f64 = 1.0;
const SYS_CALLS_PER_BUSY_MESSAGE: f64 = 0.0;
const WORKER_REPORT_FIELD_COUNT: usize = 11;
const WORKER_TIMEOUT_MS: u64 = 60_000;

const _: () = assert!(DEPTH_MINIMUM.is_power_of_two());

pub fn run_suite(options: &SuiteOptions) -> Result<SuiteReport, Warning> {
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
                        results.push(run_topology(
                            options,
                            *topology,
                            *payload_size,
                            *wait_mode,
                            *transport,
                            run_index,
                        )?);
                        run_index = run_index.saturating_add(1);
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
    let depth = depth_for(options.messages_per_publisher);
    let scenario_context = ScenarioContext {
        options,
        topology,
        transport,
        wait_mode,
        payload_size,
        depth,
        domain: &domain,
        topic: &topic,
        response_topic: &response_topic,
        ready_directory: &ready_directory,
        startup_latency_ns,
    };
    let result = match topology.kind {
        ScenarioKind::OneWay | ScenarioKind::SlowSubscriber => run_one_way(scenario_context)?,
        ScenarioKind::PingPong => run_ping_pong(scenario_context)?,
        ScenarioKind::CrashSubscriber => run_crash_subscriber(scenario_context)?,
        ScenarioKind::CrashPublisher => run_crash_publisher(scenario_context)?,
    };
    remove_ready_directory(&ready_directory)?;
    Ok(result)
}

fn run_one_way(context: ScenarioContext<'_>) -> Result<BenchmarkResult, Warning> {
    let mut subscribers = Vec::new();
    let attach_started_at = monotonic_time_ns()?;
    for subscriber_index in 0..context.topology.subscriber_count {
        let ready_file = ready_file(context.ready_directory, subscriber_index);
        subscribers.push(spawn_worker(worker_arguments(
            context,
            WorkerRole::Subscriber,
            subscriber_index,
            Some(&ready_file),
        ))?);
    }
    wait_ready_files(context.ready_directory, context.topology.subscriber_count)?;
    let attach_latency_ns = monotonic_time_ns()?.saturating_sub(attach_started_at);
    let wall_started_at = monotonic_time_ns()?;
    let mut publishers = Vec::new();
    for publisher_index in 0..context.topology.publisher_count {
        publishers.push(spawn_worker(worker_arguments(
            context,
            WorkerRole::Publisher,
            publisher_index,
            None,
        ))?);
    }
    let mut parsed_outputs = Vec::new();
    for publisher in publishers {
        parsed_outputs.push(parse_worker_output(wait_for_worker(publisher, false)?)?);
    }
    for subscriber in subscribers {
        parsed_outputs.push(parse_worker_output(wait_for_worker(subscriber, false)?)?);
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

fn run_ping_pong(context: ScenarioContext<'_>) -> Result<BenchmarkResult, Warning> {
    let ready_file = ready_file(context.ready_directory, 0);
    let attach_started_at = monotonic_time_ns()?;
    let responder = spawn_worker(worker_arguments(
        context,
        WorkerRole::PingResponder,
        0,
        Some(&ready_file),
    ))?;
    wait_ready_files(context.ready_directory, 1)?;
    let attach_latency_ns = monotonic_time_ns()?.saturating_sub(attach_started_at);
    let wall_started_at = monotonic_time_ns()?;
    let requester = spawn_worker(worker_arguments(
        context,
        WorkerRole::PingRequester,
        0,
        None,
    ))?;
    let requester_output = parse_worker_output(wait_for_worker(requester, false)?)?;
    let responder_output = parse_worker_output(wait_for_worker(responder, false)?)?;
    let wall_time_ns = monotonic_time_ns()?.saturating_sub(wall_started_at);
    aggregate_result(
        context,
        vec![requester_output.clone_report_only(), responder_output],
        requester_output.latencies,
        wall_time_ns,
        attach_latency_ns,
    )
}

fn run_crash_subscriber(context: ScenarioContext<'_>) -> Result<BenchmarkResult, Warning> {
    let ready_file = ready_file(context.ready_directory, 0);
    let crash_subscriber = spawn_worker(worker_arguments(
        context,
        WorkerRole::CrashSubscriber,
        0,
        Some(&ready_file),
    ))?;
    wait_ready_files(context.ready_directory, 1)?;
    let wall_started_at = monotonic_time_ns()?;
    let publisher = spawn_worker(worker_arguments(context, WorkerRole::Publisher, 0, None))?;
    let publisher_output = parse_worker_output(wait_for_worker(publisher, false)?)?;
    let _crash_output = wait_for_worker(crash_subscriber, true)?;
    let wall_time_ns = monotonic_time_ns()?.saturating_sub(wall_started_at);
    aggregate_result(context, vec![publisher_output], Vec::new(), wall_time_ns, 0)
}

fn run_crash_publisher(context: ScenarioContext<'_>) -> Result<BenchmarkResult, Warning> {
    let ready_file = ready_file(context.ready_directory, 0);
    let subscriber = spawn_worker(worker_arguments(
        context,
        WorkerRole::Subscriber,
        0,
        Some(&ready_file),
    ))?;
    wait_ready_files(context.ready_directory, 1)?;
    let wall_started_at = monotonic_time_ns()?;
    let crash_publisher = spawn_worker(worker_arguments(
        context,
        WorkerRole::CrashPublisher,
        0,
        None,
    ))?;
    let _crash_output = wait_for_worker(crash_publisher, true)?;
    let subscriber_output = parse_worker_output(wait_for_worker(subscriber, false)?)?;
    let wall_time_ns = monotonic_time_ns()?.saturating_sub(wall_started_at);
    aggregate_result(
        context,
        vec![subscriber_output],
        Vec::new(),
        wall_time_ns,
        0,
    )
}

fn aggregate_result(
    context: ScenarioContext<'_>,
    outputs: Vec<ParsedWorkerOutput>,
    round_trip_samples: Vec<u64>,
    wall_time_ns: u64,
    attach_latency_ns: u64,
) -> Result<BenchmarkResult, Warning> {
    let mut latency_samples = Vec::new();
    let mut reports = Vec::new();
    for output in outputs {
        latency_samples.extend(output.latencies);
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
    let mut round_trip_samples = round_trip_samples;
    Ok(BenchmarkResult::from_samples(
        BenchmarkResultContext {
            scenario: context.topology.name,
            transport: context.transport,
            wait_mode: context.wait_mode,
            payload_size: context.payload_size,
            publisher_count: context.topology.publisher_count,
            subscriber_count: context.topology.subscriber_count,
            measured_messages: measured_messages(context),
            wall_time_ns,
            allocation_count: aggregate.allocations,
            cpu_time_ns: aggregate.cpu_time_ns,
            rss_bytes: aggregate.rss_bytes,
            pss_bytes: aggregate.pss_bytes,
            context_switches: aggregate.context_switches,
            syscalls_per_message: syscall_metric(context.wait_mode),
            warning_count: aggregate.warnings,
            skipped_messages: aggregate.skipped_messages,
            backpressure_pinned_slot: aggregate.backpressure_pinned_slot,
            startup_latency_ns: context.startup_latency_ns,
            attach_latency_ns,
        },
        &mut latency_samples,
        &mut round_trip_samples,
    ))
}

fn worker_arguments(
    context: ScenarioContext<'_>,
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
        context.options.messages_per_publisher.to_string(),
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
    let started_at = monotonic_time_ns()?;
    loop {
        if child
            .try_wait()
            .map_err(|error| warning_from_io(error, "wait_for_worker::try_wait"))?
            .is_some()
        {
            break;
        }
        if monotonic_time_ns()?.saturating_sub(started_at)
            > Duration::from_millis(WORKER_TIMEOUT_MS).as_nanos() as u64
        {
            child
                .kill()
                .map_err(|error| warning_from_io(error, "wait_for_worker::kill"))?;
            return Err(warning("wait_for_worker::timeout"));
        }
        std::thread::sleep(Duration::from_millis(READY_WAIT_POLL_INTERVAL_MS));
    }
    let output = child
        .wait_with_output()
        .map_err(|error| warning_from_io(error, "wait_for_worker::output"))?;
    if !output.status.success() && !allow_failure {
        print_child_stderr(&output);
        return Err(warning("wait_for_worker::status"));
    }
    if output.status.success() && allow_failure {
        return Err(warning("wait_for_worker::expected_crash"));
    }
    Ok(output)
}

fn parse_worker_output(output: Output) -> Result<ParsedWorkerOutput, Warning> {
    if !output.stderr.is_empty() {
        print_child_stderr(&output);
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| warning("parse_worker_output::stdout_utf8"))?;
    let mut parsed = ParsedWorkerOutput {
        report: None,
        latencies: Vec::new(),
    };
    for line in stdout.lines() {
        if let Some(value) = line.strip_prefix(LATENCY_PREFIX) {
            parsed
                .latencies
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

fn measured_messages(context: ScenarioContext<'_>) -> u64 {
    match context.topology.kind {
        ScenarioKind::PingPong => u64::from(context.options.messages_per_publisher),
        ScenarioKind::CrashSubscriber | ScenarioKind::CrashPublisher => {
            u64::from(context.options.messages_per_publisher)
        }
        ScenarioKind::OneWay | ScenarioKind::SlowSubscriber => {
            u64::from(context.topology.publisher_count)
                * u64::from(context.topology.subscriber_count)
                * u64::from(context.options.messages_per_publisher)
        }
    }
}

fn depth_for(messages_per_publisher: u32) -> usize {
    let total_messages = messages_per_publisher
        .saturating_add(warmup_messages_per_publisher())
        .saturating_add(1);
    DEPTH_MINIMUM.max(total_messages.next_power_of_two() as usize)
}

fn syscall_metric(wait_mode: WaitMode) -> f64 {
    if wait_mode.uses_blocking_wait() {
        SYS_CALLS_PER_BLOCKING_MESSAGE
    } else {
        SYS_CALLS_PER_BUSY_MESSAGE
    }
}

fn selected_topologies(options: &SuiteOptions) -> &'static [Topology] {
    if options.matrix_smoke {
        &TOPOLOGIES[..1]
    } else {
        TOPOLOGIES
    }
}

fn selected_payload_sizes(options: &SuiteOptions) -> &'static [usize] {
    if options.matrix_smoke {
        &PAYLOAD_SIZES[..1]
    } else {
        PAYLOAD_SIZES
    }
}

fn selected_wait_modes(options: &SuiteOptions) -> &'static [WaitMode] {
    if options.matrix_smoke {
        &WAIT_MODES[..1]
    } else {
        WAIT_MODES
    }
}

fn ready_directory(run_index: usize) -> PathBuf {
    std::env::temp_dir().join(format!("rbus-bench-{}-{run_index}", std::process::id()))
}

fn ready_file(directory: &Path, index: u8) -> PathBuf {
    directory.join(format!("subscriber-{index}.ready"))
}

fn create_ready_directory(directory: &Path) -> Result<(), Warning> {
    std::fs::create_dir_all(directory).map_err(|error| warning_from_io(error, "ready_dir::create"))
}

fn remove_ready_directory(directory: &Path) -> Result<(), Warning> {
    if directory.exists() {
        std::fs::remove_dir_all(directory)
            .map_err(|error| warning_from_io(error, "ready_dir::remove"))?;
    }
    Ok(())
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

#[derive(Clone, Debug)]
struct ParsedWorkerOutput {
    report: Option<WorkerReport>,
    latencies: Vec<u64>,
}

impl ParsedWorkerOutput {
    fn clone_report_only(&self) -> Self {
        Self {
            report: self.report,
            latencies: Vec::new(),
        }
    }
}

#[derive(Clone, Copy)]
struct ScenarioContext<'a> {
    options: &'a SuiteOptions,
    topology: Topology,
    transport: TransportKind,
    wait_mode: WaitMode,
    payload_size: usize,
    depth: usize,
    domain: &'a str,
    topic: &'a str,
    response_topic: &'a str,
    ready_directory: &'a Path,
    startup_latency_ns: u64,
}
