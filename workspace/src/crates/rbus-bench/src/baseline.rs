use std::fs;
use std::path::Path;

use rbus_core::warning::{Warning, WarningCode};

use crate::metrics::BenchmarkResult;

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const BASELINE_FIELD_COUNT: usize = 9;
const FIRST_DATA_LINE: usize = 2;
const LATENCY_GATE_NUMERATOR: u64 = 105;
const LATENCY_GATE_DENOMINATOR: u64 = 100;
const THROUGHPUT_GATE_FACTOR: f64 = 0.95;
const WILDCARD: &str = "*";
const WILDCARD_PAYLOAD_SIZE: usize = 0;

/// Benchmark metadata
pub fn enforce_regression_gates(
    baseline_path: &Path,
    results: &[BenchmarkResult],
) -> Result<(), Warning> {
    let entries = read_baseline_entries(baseline_path)?;
    for result in results {
        let baseline = entries
            .iter()
            .find(|entry| entry.matches(result))
            .ok_or_else(|| gate_warning("enforce_regression_gates::missing"))?;
        enforce_latency_gate(result.latency_p50_ns, baseline.latency_p50_ns, "p50")?;
        enforce_latency_gate(result.latency_p99_ns, baseline.latency_p99_ns, "p99")?;
        if result.throughput_messages_per_second
            < baseline.throughput_messages_per_second * THROUGHPUT_GATE_FACTOR
        {
            return Err(gate_warning("enforce_regression_gates::throughput"));
        }
        if result.allocations_per_message > baseline.allocations_per_message {
            return Err(gate_warning("enforce_regression_gates::allocations"));
        }
        if result.syscalls_per_message > baseline.syscalls_per_message {
            return Err(gate_warning("enforce_regression_gates::syscalls"));
        }
        if result.warning_count > baseline.warning_count {
            return Err(gate_warning("enforce_regression_gates::warnings"));
        }
    }
    Ok(())
}

fn read_baseline_entries(path: &Path) -> Result<Vec<BaselineEntry>, Warning> {
    let contents = fs::read_to_string(path)
        .map_err(|error| warning_from_io(error, "read_baseline_entries::read"))?;
    let mut entries = Vec::new();
    for (line_index, line) in contents.lines().enumerate().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        entries.push(parse_baseline_entry(line, line_index + FIRST_DATA_LINE)?);
    }
    if entries.is_empty() {
        return Err(gate_warning("read_baseline_entries::empty"));
    }
    Ok(entries)
}

fn parse_baseline_entry(line: &str, _line_number: usize) -> Result<BaselineEntry, Warning> {
    let fields: Vec<&str> = line.split(',').collect();
    if fields.len() != BASELINE_FIELD_COUNT {
        return Err(gate_warning("parse_baseline_entry::field_count"));
    }
    Ok(BaselineEntry {
        scenario: fields[0].to_owned(),
        transport: fields[1].to_owned(),
        wait_mode: fields[2].to_owned(),
        payload_size: parse_field(fields[3], "payload_size")?,
        latency_p50_ns: parse_field(fields[4], "latency_p50_ns")?,
        latency_p99_ns: parse_field(fields[5], "latency_p99_ns")?,
        throughput_messages_per_second: parse_field(fields[6], "throughput_messages_per_second")?,
        allocations_per_message: parse_field(fields[7], "allocations_per_message")?,
        syscalls_per_message: parse_field(fields[8], "syscalls_per_message")?,
        warning_count: 0,
    })
}

fn parse_field<T>(value: &str, _field_name: &'static str) -> Result<T, Warning>
where
    T: core::str::FromStr,
{
    value
        .parse::<T>()
        .map_err(|_| gate_warning("parse_baseline_entry::parse"))
}

fn enforce_latency_gate(actual: u64, baseline: u64, _metric: &'static str) -> Result<(), Warning> {
    let threshold = baseline.saturating_mul(LATENCY_GATE_NUMERATOR) / LATENCY_GATE_DENOMINATOR;
    if actual > threshold {
        return Err(gate_warning("enforce_regression_gates::latency"));
    }
    Ok(())
}

fn gate_warning(operation: &'static str) -> Warning {
    Warning::new(WarningCode::InvalidConfig, COMPONENT_RBUS_BENCH, operation)
}

fn warning_from_io(error: std::io::Error, operation: &'static str) -> Warning {
    match error.raw_os_error() {
        Some(errno) => Warning::new(WarningCode::SyscallFailed, COMPONENT_RBUS_BENCH, operation)
            .with_os_errno(errno),
        None => Warning::new(WarningCode::SyscallFailed, COMPONENT_RBUS_BENCH, operation),
    }
}

struct BaselineEntry {
    scenario: String,
    transport: String,
    wait_mode: String,
    payload_size: usize,
    latency_p50_ns: u64,
    latency_p99_ns: u64,
    throughput_messages_per_second: f64,
    allocations_per_message: f64,
    syscalls_per_message: f64,
    warning_count: u64,
}

impl BaselineEntry {
    fn matches(&self, result: &BenchmarkResult) -> bool {
        string_matches(&self.scenario, result.scenario)
            && string_matches(&self.transport, result.transport.as_str())
            && string_matches(&self.wait_mode, result.wait_mode.as_str())
            && (self.payload_size == WILDCARD_PAYLOAD_SIZE
                || self.payload_size == result.payload_size)
    }
}

fn string_matches(pattern: &str, value: &str) -> bool {
    pattern == WILDCARD || pattern == value
}
