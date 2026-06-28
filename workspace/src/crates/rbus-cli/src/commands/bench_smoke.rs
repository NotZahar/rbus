use std::path::PathBuf;
use std::process::{Command, ExitCode};

use crate::args::CliArguments;
use crate::control_client::print_result;

use rbus_core::warning::{Warning, WarningCode};

const BENCHMARK_BINARY_NAME: &str = "rbus-bench";
const BENCHMARK_SMOKE_ARGUMENT: &str = "--smoke";
const COMPONENT_CLI: &str = "rbus-cli";
const JSON_CONTROL_LIMIT: u32 = 0x20;
const WARNING_EXIT_CODE: i32 = 2;

pub fn run(arguments: &CliArguments) -> ExitCode {
    print_result(run_benchmark_smoke(arguments))
}

fn run_benchmark_smoke(arguments: &CliArguments) -> Result<String, Warning> {
    let report = benchmark_smoke_report()?;
    if arguments.json {
        Ok(json_wrapped_report(&arguments.domain, &report))
    } else {
        Ok(report)
    }
}

fn benchmark_smoke_report() -> Result<String, Warning> {
    let output = Command::new(benchmark_binary_path()?)
        .arg(BENCHMARK_SMOKE_ARGUMENT)
        .output()
        .map_err(|error| warning_from_io(error, "benchmark_smoke_report::spawn"))?;
    if output.status.code() != Some(0) {
        return Err(Warning::new(
            WarningCode::InternalInvariantWarning,
            COMPONENT_CLI,
            if output.status.code() == Some(WARNING_EXIT_CODE) {
                "benchmark_smoke_report::warning_status"
            } else {
                "benchmark_smoke_report::status"
            },
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|_| {
        Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_CLI,
            "benchmark_smoke_report::utf8",
        )
    })?;
    let report = stdout.trim();
    if is_smoke_report(report) {
        Ok(report.to_owned())
    } else {
        Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_CLI,
            "benchmark_smoke_report::shape",
        ))
    }
}

fn benchmark_binary_path() -> Result<PathBuf, Warning> {
    let current_executable =
        std::env::current_exe().map_err(|error| warning_from_io(error, "current_exe"))?;
    let binary_directory = current_executable.parent().ok_or_else(|| {
        Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_CLI,
            "benchmark_binary_path::parent",
        )
    })?;
    Ok(binary_directory.join(BENCHMARK_BINARY_NAME))
}

fn is_smoke_report(report: &str) -> bool {
    report.starts_with('{')
        && report.ends_with('}')
        && report.contains("\"status\":\"ok\"")
        && report.contains("\"warning_count\":0")
        && report.contains("\"scenario_count\":")
        && report.contains("\"transport_count\":")
        && report.contains("\"payload_size_count\":")
        && report.contains("\"blocking_wait_wake_latency_ns\":")
}

fn json_wrapped_report(domain: &str, report: &str) -> String {
    let mut output = String::new();
    output.push_str("{\"domain\":");
    push_json_string(&mut output, domain);
    output.push_str(",\"bench_smoke\":");
    output.push_str(report);
    output.push('}');
    output
}

fn push_json_string(output: &mut String, value: &str) {
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            value if u32::from(value) < JSON_CONTROL_LIMIT => {
                output.push_str(&format!("\\u{:04x}", u32::from(value)));
            }
            value => output.push(value),
        }
    }
    output.push('"');
}

fn warning_from_io(error: std::io::Error, operation: &'static str) -> Warning {
    match error.raw_os_error() {
        Some(errno) => {
            Warning::new(WarningCode::SyscallFailed, COMPONENT_CLI, operation).with_os_errno(errno)
        }
        None => Warning::new(WarningCode::SyscallFailed, COMPONENT_CLI, operation),
    }
}
