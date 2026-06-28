use std::io::{StdoutLock, Write};
use std::path::Path;

use rbus_core::warning::{Warning, WarningCode};

use crate::args::SuiteOptions;
use crate::baseline::enforce_regression_gates;
use crate::daemon_process::DaemonProcess;
use crate::load_mode::LoadMode;
use crate::metrics::{BenchmarkResult, BenchmarkResultContext, BenchmarkSampleSets};
use crate::output_format::OutputFormat;
use crate::report::{CSV_HEADER, SuiteReport, result_csv_line};
use crate::suite_matrix::{
    depth_for, measured_messages, messages_per_publisher_for_payload, selected_payload_sizes,
    selected_topologies, selected_transports, selected_wait_modes,
};
use crate::suite_process::{
    parse_worker_output, spawn_worker, wait_for_worker, wait_worker_ready_files,
};
use crate::suite_ready::{
    attach_ready_file, create_ready_directory, ready_directory, remove_ready_directory, start_file,
    warmup_start_file, worker_ready_file, write_start_file, write_warmup_start_file,
};
use crate::suite_repetition::aggregate_repeated_results;
use crate::suite_worker_output::ParsedWorkerOutput;
use crate::syscall_measurement::{SyscallMeasurementContext, measure_syscalls_per_message};
use crate::system_metrics::monotonic_time_ns;
use crate::topology::{ScenarioKind, Topology};
use crate::transport_kind::TransportKind;
use crate::wait_mode::WaitMode;
use crate::worker::{WorkerReport, WorkerRole};

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const SUITE_DOMAIN_PREFIX: &str = "rbus-suite";
const SUITE_TIMEOUT_MINUTES: u64 = 20;
const SECONDS_PER_MINUTE: u64 = 60;
const NANOS_PER_SECOND: u64 = 1_000_000_000;
const NANOS_PER_MILLISECOND: u64 = 1_000_000;
const BYTES_PER_GIB: f64 = 1024.0 * 1024.0 * 1024.0;
const MAX_CONFIGURED_ACTUAL_WRITE_GIB_PER_SECOND: f64 = 8.0;
const MAX_CONFIGURED_EFFECTIVE_DELIVERY_GIB_PER_SECOND: f64 = 64.0;
const PACED_RATE_TOLERANCE_RATIO: f64 = 0.01;
const MAX_LOW_RATE_BACKLOG_P99_MESSAGES: u64 = 1;
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
    let transports = selected_transports(options);
    if topologies.is_empty()
        || payload_sizes.is_empty()
        || wait_modes.is_empty()
        || transports.is_empty()
    {
        return Err(warning("run_suite::empty_matrix"));
    }
    let mut results = Vec::new();
    let mut run_index = 0_usize;

    for topology in &topologies {
        for payload_size in &payload_sizes {
            for wait_mode in &wait_modes {
                for transport in &transports {
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
                        if options.gate_enabled {
                            validate_paced_result(&result)?;
                        }
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

fn validate_paced_result(result: &BenchmarkResult) -> Result<(), Warning> {
    if result.load_mode == LoadMode::Saturated {
        return Ok(());
    }
    if result.warning_count != 0 {
        return Err(warning("paced_gate::warnings"));
    }
    if result.skipped_messages != 0 {
        return Err(warning("paced_gate::skipped_messages"));
    }
    if result.backpressure_pinned_slot != 0 {
        return Err(warning("paced_gate::backpressure"));
    }
    if result.transport.is_rbus()
        && result.wait_mode == WaitMode::BusyPoll
        && result.syscalls_per_message > 0.0
    {
        return Err(warning("paced_gate::syscalls"));
    }
    if matches!(
        result.transport,
        TransportKind::RbusLoanedMetadataOnly | TransportKind::RbusLoanedFill
    ) && result.allocations_per_message > 0.0
    {
        return Err(warning("paced_gate::allocations"));
    }
    let configured_rate = f64::from(result.publisher_count) * result.rate_hz;
    let rate_delta = (result.unique_publish_messages_per_second - configured_rate).abs();
    if rate_delta > configured_rate * PACED_RATE_TOLERANCE_RATIO {
        return Err(warning("paced_gate::rate"));
    }
    if result.scenario == "one-publisher-one-subscriber"
        && result.transport.is_rbus()
        && result.ring_backlog_p99_messages > MAX_LOW_RATE_BACKLOG_P99_MESSAGES
    {
        return Err(warning("paced_gate::backlog"));
    }
    Ok(())
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
    validate_configured_workload(options, topology, payload_size)?;
    let depth = depth_for(payload_size, messages_per_publisher);
    let scenario_context = ScenarioContext {
        topology,
        transport,
        wait_mode,
        payload_size,
        depth,
        messages_per_publisher,
        load_mode: options.load_mode,
        rate_hz: options.rate_hz,
        duration_ms: options.duration_ms,
        pace_spin_tail_us: options.pace_spin_tail_us,
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

fn validate_configured_workload(
    options: &SuiteOptions,
    topology: Topology,
    payload_size: usize,
) -> Result<(), Warning> {
    if options.load_mode == LoadMode::Saturated {
        return Ok(());
    }
    let rate_hz = options.rate_hz.ok_or_else(|| warning("paced_rate_hz"))?;
    let actual_write_gib_per_second =
        f64::from(topology.publisher_count) * rate_hz * payload_size as f64 / BYTES_PER_GIB;
    let effective_delivery_gib_per_second =
        actual_write_gib_per_second * f64::from(topology.subscriber_count);
    if actual_write_gib_per_second > MAX_CONFIGURED_ACTUAL_WRITE_GIB_PER_SECOND
        || effective_delivery_gib_per_second > MAX_CONFIGURED_EFFECTIVE_DELIVERY_GIB_PER_SECOND
    {
        return Err(warning("paced_workload_safety_cap"));
    }
    Ok(())
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
    let mut publish_lateness_samples = Vec::new();
    let mut inter_publish_gap_samples = Vec::new();
    let mut inter_receive_gap_samples = Vec::new();
    let mut ring_backlog_samples = Vec::new();
    let mut reports = Vec::new();
    for output in outputs {
        transport_latency_samples.extend(output.transport_latencies);
        end_to_end_latency_samples.extend(output.end_to_end_latencies);
        round_trip_samples.extend(output.round_trip_latencies);
        publish_lateness_samples.extend(output.publish_lateness);
        inter_publish_gap_samples.extend(output.inter_publish_gaps);
        inter_receive_gap_samples.extend(output.inter_receive_gaps);
        ring_backlog_samples.extend(output.ring_backlog_messages);
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
            load_mode: context.load_mode,
            rate_hz: context.rate_hz.unwrap_or(0.0),
            rate_scope: crate::load_mode::RateScope::PerPublisher,
            duration_ms: context.duration_ms.unwrap_or(0),
            configured_messages_per_publisher: context.messages_per_publisher,
            actual_messages_per_publisher: context.messages_per_publisher,
            measured_messages: measured_messages(
                context.topology.kind,
                context.topology.publisher_count,
                context.topology.subscriber_count,
                context.messages_per_publisher,
            ),
            unique_published_messages: u64::from(context.topology.publisher_count)
                * u64::from(context.messages_per_publisher),
            measured_window_ns: measured_window_ns(context, aggregate.duration_ns),
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
            publish_deadline_miss_count: aggregate.publish_deadline_miss_count,
            backlog_metric_available: context.transport.is_rbus(),
            startup_latency_ns: context.startup_latency_ns,
            attach_latency_ns,
        },
        BenchmarkSampleSets {
            transport_latency_samples: &mut transport_latency_samples,
            end_to_end_latency_samples: &mut end_to_end_latency_samples,
            round_trip_samples: &mut round_trip_samples,
            publish_lateness_samples: &mut publish_lateness_samples,
            inter_publish_gap_samples: &mut inter_publish_gap_samples,
            inter_receive_gap_samples: &mut inter_receive_gap_samples,
            ring_backlog_samples: &mut ring_backlog_samples,
        },
    ))
}

fn measured_window_ns(context: ScenarioContext<'_>, worker_duration_ns: u64) -> u64 {
    match context.load_mode {
        LoadMode::Saturated => worker_duration_ns,
        LoadMode::Paced => context
            .duration_ms
            .unwrap_or(0)
            .saturating_mul(NANOS_PER_MILLISECOND),
    }
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
        String::from("--load-mode"),
        context.load_mode.as_str().to_owned(),
        String::from("--wait-mode"),
        context.wait_mode.as_str().to_owned(),
        String::from("--transport"),
        context.transport.as_str().to_owned(),
    ];
    if let Some(rate_hz) = context.rate_hz {
        arguments.push(String::from("--rate-hz"));
        arguments.push(format!("{rate_hz:.9}"));
    }
    if context.pace_spin_tail_us != 0 {
        arguments.push(String::from("--pace-spin-tail-us"));
        arguments.push(context.pace_spin_tail_us.to_string());
    }
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
        aggregate.publish_deadline_miss_count = aggregate
            .publish_deadline_miss_count
            .saturating_add(report.publish_deadline_miss_count);
    }
    aggregate
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
    load_mode: crate::load_mode::LoadMode,
    rate_hz: Option<f64>,
    duration_ms: Option<u64>,
    pace_spin_tail_us: u64,
    domain: &'a str,
    topic: &'a str,
    response_topic: &'a str,
    ready_directory: &'a Path,
    startup_latency_ns: u64,
    run_index: usize,
    syscalls_per_message: Option<f64>,
}
