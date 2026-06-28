use std::process::ExitCode;

use crate::args::{BenchArguments, BenchMode};
use crate::blocking_wait::blocking_wait_wake_latency_ns;
use crate::hot_path_syscall::run_probe;
use crate::metrics::MetricsSnapshot;
use crate::payload::PAYLOAD_SIZES;
use crate::pinning::pinning_enabled;
use crate::process::benchmark_process_id;
use crate::report::{SmokeReport, SmokeReportContext};
use crate::scenarios::SCENARIO_NAMES;
use crate::suite::{print_suite_report, run_suite};
use crate::topology::TOPOLOGIES;
use crate::transports::TRANSPORT_NAMES;
use crate::worker::run_worker;

const WARNING_EXIT_CODE: u8 = 2;

/// Benchmark metadata
pub fn run(arguments: BenchArguments) -> ExitCode {
    match arguments.mode {
        BenchMode::Help => ExitCode::SUCCESS,
        BenchMode::Smoke => run_smoke(),
        BenchMode::Suite(options) => match run_suite(&options) {
            Ok(report) => {
                print_suite_report(&report, options.output_format);
                ExitCode::SUCCESS
            }
            Err(warning) => warning_exit(warning),
        },
        BenchMode::HotPathSyscallProbe(operation) => match run_probe(operation) {
            Ok(report) => {
                println!("{}", report.to_json());
                ExitCode::SUCCESS
            }
            Err(warning) => warning_exit(warning),
        },
        BenchMode::SuiteWorker(worker_arguments) => match run_worker(worker_arguments) {
            Ok(()) => ExitCode::SUCCESS,
            Err(warning) => warning_exit(warning),
        },
        BenchMode::Missing => {
            println!("{{\"status\":\"warning\",\"message\":\"specify --smoke or --suite\"}}");
            ExitCode::from(WARNING_EXIT_CODE)
        }
    }
}

fn run_smoke() -> ExitCode {
    let topology = TOPOLOGIES[0];
    let metrics = match blocking_wait_wake_latency_ns() {
        Ok(blocking_wait_wake_latency_ns) => MetricsSnapshot {
            messages_per_second: 0,
            blocking_wait_wake_latency_ns,
        },
        Err(warning) => return warning_exit(warning),
    };
    let context = SmokeReportContext {
        scenario_count: SCENARIO_NAMES.len(),
        transport_count: TRANSPORT_NAMES.len(),
        payload_size_count: PAYLOAD_SIZES.len(),
        process_id: benchmark_process_id(),
        pinning_enabled: pinning_enabled(),
        publisher_count: topology.publisher_count,
        subscriber_count: topology.subscriber_count,
        messages_per_second: metrics.messages_per_second,
        blocking_wait_wake_latency_ns: metrics.blocking_wait_wake_latency_ns,
    };
    let report = SmokeReport::new("ok", 0, context);
    println!("{}", report.to_json());
    ExitCode::SUCCESS
}

fn warning_exit(warning: rbus_core::warning::Warning) -> ExitCode {
    eprintln!(
        "warning [{}] {}::{}: {}\nstacktrace:\n{}",
        warning.code().as_str(),
        warning.component(),
        warning.operation(),
        warning.message(),
        std::backtrace::Backtrace::force_capture()
    );
    ExitCode::from(WARNING_EXIT_CODE)
}
