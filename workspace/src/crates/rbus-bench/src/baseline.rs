use std::fs;
use std::path::Path;

use rbus_core::warning::{Warning, WarningCode};

use crate::metrics::BenchmarkResult;
use crate::payload::PAYLOAD_SIZES;
use crate::report::CSV_HEADER;

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const BACKPRESSURE_PINNED_SLOT_FIELD: usize = 32;
const BASELINE_FIELD_COUNT: usize = 36;
const TRANSPORT_LATENCY_P50_FIELD: usize = 8;
const TRANSPORT_LATENCY_P90_FIELD: usize = 9;
const TRANSPORT_LATENCY_P99_FIELD: usize = 10;
const TRANSPORT_LATENCY_P999_FIELD: usize = 11;
const TRANSPORT_LATENCY_P9999_FIELD: usize = 12;
const END_TO_END_LATENCY_P50_FIELD: usize = 13;
const END_TO_END_LATENCY_P90_FIELD: usize = 14;
const END_TO_END_LATENCY_P99_FIELD: usize = 15;
const END_TO_END_LATENCY_P999_FIELD: usize = 16;
const END_TO_END_LATENCY_P9999_FIELD: usize = 17;
const LATENCY_GATE_NUMERATOR: u64 = 105;
const LATENCY_GATE_DENOMINATOR: u64 = 100;
const MEASURED_MESSAGES_FIELD: usize = 7;
const PAYLOAD_SIZE_FIELD: usize = 4;
const PAYLOAD_THROUGHPUT_FIELD: usize = 23;
const PUBLISHER_COUNT_FIELD: usize = 5;
const ROUND_TRIP_P50_FIELD: usize = 18;
const ROUND_TRIP_P90_FIELD: usize = 19;
const ROUND_TRIP_P99_FIELD: usize = 20;
const ROUND_TRIP_P999_FIELD: usize = 21;
const SCENARIO_FIELD: usize = 1;
const SKIPPED_MESSAGES_FIELD: usize = 31;
const STATUS_FIELD: usize = 0;
const STATUS_OK: &str = "ok";
const SUBSCRIBER_COUNT_FIELD: usize = 6;
const SYSCALLS_FIELD: usize = 28;
const THROUGHPUT_GATE_FACTOR: f64 = 0.95;
const THROUGHPUT_FIELD: usize = 22;
const TRANSPORT_FIELD: usize = 2;
const WAIT_MODE_FIELD: usize = 3;
const WARNING_COUNT_FIELD: usize = 30;
const ALLOCATIONS_FIELD: usize = 29;
const WILDCARD_KEY: &str = "*";

/// Benchmark baseline path.
pub const DEFAULT_BASELINE_PATH: &str = "benches/baselines/rbus_suite_baseline_x86_64.csv";

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
        enforce_latency_gate(
            result.transport_latency_p50_ns,
            baseline.transport_latency_p50_ns,
            "transport_p50",
        )?;
        enforce_latency_gate(
            result.transport_latency_p90_ns,
            baseline.transport_latency_p90_ns,
            "transport_p90",
        )?;
        enforce_latency_gate(
            result.transport_latency_p99_ns,
            baseline.transport_latency_p99_ns,
            "transport_p99",
        )?;
        enforce_latency_gate(
            result.transport_latency_p999_ns,
            baseline.transport_latency_p999_ns,
            "transport_p999",
        )?;
        enforce_latency_gate(
            result.transport_latency_p9999_ns,
            baseline.transport_latency_p9999_ns,
            "transport_p9999",
        )?;
        enforce_latency_gate(
            result.end_to_end_latency_p50_ns,
            baseline.end_to_end_latency_p50_ns,
            "end_to_end_p50",
        )?;
        enforce_latency_gate(
            result.end_to_end_latency_p90_ns,
            baseline.end_to_end_latency_p90_ns,
            "end_to_end_p90",
        )?;
        enforce_latency_gate(
            result.end_to_end_latency_p99_ns,
            baseline.end_to_end_latency_p99_ns,
            "end_to_end_p99",
        )?;
        enforce_latency_gate(
            result.end_to_end_latency_p999_ns,
            baseline.end_to_end_latency_p999_ns,
            "end_to_end_p999",
        )?;
        enforce_latency_gate(
            result.end_to_end_latency_p9999_ns,
            baseline.end_to_end_latency_p9999_ns,
            "end_to_end_p9999",
        )?;
        enforce_latency_gate(
            result.round_trip_p50_ns,
            baseline.round_trip_p50_ns,
            "rt_p50",
        )?;
        enforce_latency_gate(
            result.round_trip_p90_ns,
            baseline.round_trip_p90_ns,
            "rt_p90",
        )?;
        enforce_latency_gate(
            result.round_trip_p99_ns,
            baseline.round_trip_p99_ns,
            "rt_p99",
        )?;
        enforce_latency_gate(
            result.round_trip_p999_ns,
            baseline.round_trip_p999_ns,
            "rt_p999",
        )?;
        enforce_throughput_gate(
            result.throughput_messages_per_second,
            baseline.throughput_messages_per_second,
        )?;
        enforce_payload_throughput_gate(
            result.payload_throughput_gib_per_second,
            baseline.payload_throughput_gib_per_second,
        )?;
        enforce_float_ceiling(
            result.allocations_per_message,
            baseline.allocations_per_message,
            "enforce_regression_gates::allocations",
        )?;
        enforce_float_ceiling(
            result.syscalls_per_message,
            baseline.syscalls_per_message,
            "enforce_regression_gates::syscalls",
        )?;
        enforce_counter_ceiling(
            result.warning_count,
            baseline.warning_count,
            "enforce_regression_gates::warnings",
        )?;
        enforce_counter_ceiling(
            result.skipped_messages,
            baseline.skipped_messages,
            "enforce_regression_gates::skipped_messages",
        )?;
        enforce_counter_ceiling(
            result.backpressure_pinned_slot,
            baseline.backpressure_pinned_slot,
            "enforce_regression_gates::backpressure",
        )?;
    }
    Ok(())
}

fn read_baseline_entries(path: &Path) -> Result<Vec<BaselineEntry>, Warning> {
    let contents = fs::read_to_string(path)
        .map_err(|error| warning_from_io(error, "read_baseline_entries::read"))?;
    let mut lines = contents.lines();
    let header = lines
        .next()
        .ok_or_else(|| gate_warning("read_baseline_entries::header"))?;
    if header != CSV_HEADER {
        return Err(gate_warning("read_baseline_entries::header"));
    }
    let mut entries = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let entry = parse_baseline_entry(line)?;
        if entries.iter().any(|existing| entry.has_same_key(existing)) {
            return Err(gate_warning("read_baseline_entries::duplicate"));
        }
        entries.push(entry);
    }
    if entries.is_empty() {
        return Err(gate_warning("read_baseline_entries::empty"));
    }
    Ok(entries)
}

fn parse_baseline_entry(line: &str) -> Result<BaselineEntry, Warning> {
    let fields: Vec<&str> = line.split(',').collect();
    if fields.len() != BASELINE_FIELD_COUNT {
        return Err(gate_warning("parse_baseline_entry::field_count"));
    }
    if fields[STATUS_FIELD] != STATUS_OK {
        return Err(gate_warning("parse_baseline_entry::status"));
    }
    Ok(BaselineEntry {
        scenario: parse_key_field(fields[SCENARIO_FIELD], "scenario")?,
        transport: parse_key_field(fields[TRANSPORT_FIELD], "transport")?,
        wait_mode: parse_key_field(fields[WAIT_MODE_FIELD], "wait_mode")?,
        payload_size: parse_payload_size_field(fields[PAYLOAD_SIZE_FIELD])?,
        publisher_count: parse_field(fields[PUBLISHER_COUNT_FIELD], "publisher_count")?,
        subscriber_count: parse_field(fields[SUBSCRIBER_COUNT_FIELD], "subscriber_count")?,
        measured_messages: parse_field(fields[MEASURED_MESSAGES_FIELD], "measured_messages")?,
        transport_latency_p50_ns: parse_field(
            fields[TRANSPORT_LATENCY_P50_FIELD],
            "transport_latency_p50_ns",
        )?,
        transport_latency_p90_ns: parse_field(
            fields[TRANSPORT_LATENCY_P90_FIELD],
            "transport_latency_p90_ns",
        )?,
        transport_latency_p99_ns: parse_field(
            fields[TRANSPORT_LATENCY_P99_FIELD],
            "transport_latency_p99_ns",
        )?,
        transport_latency_p999_ns: parse_field(
            fields[TRANSPORT_LATENCY_P999_FIELD],
            "transport_latency_p999_ns",
        )?,
        transport_latency_p9999_ns: parse_field(
            fields[TRANSPORT_LATENCY_P9999_FIELD],
            "transport_latency_p9999_ns",
        )?,
        end_to_end_latency_p50_ns: parse_field(
            fields[END_TO_END_LATENCY_P50_FIELD],
            "end_to_end_latency_p50_ns",
        )?,
        end_to_end_latency_p90_ns: parse_field(
            fields[END_TO_END_LATENCY_P90_FIELD],
            "end_to_end_latency_p90_ns",
        )?,
        end_to_end_latency_p99_ns: parse_field(
            fields[END_TO_END_LATENCY_P99_FIELD],
            "end_to_end_latency_p99_ns",
        )?,
        end_to_end_latency_p999_ns: parse_field(
            fields[END_TO_END_LATENCY_P999_FIELD],
            "end_to_end_latency_p999_ns",
        )?,
        end_to_end_latency_p9999_ns: parse_field(
            fields[END_TO_END_LATENCY_P9999_FIELD],
            "end_to_end_latency_p9999_ns",
        )?,
        round_trip_p50_ns: parse_field(fields[ROUND_TRIP_P50_FIELD], "round_trip_p50_ns")?,
        round_trip_p90_ns: parse_field(fields[ROUND_TRIP_P90_FIELD], "round_trip_p90_ns")?,
        round_trip_p99_ns: parse_field(fields[ROUND_TRIP_P99_FIELD], "round_trip_p99_ns")?,
        round_trip_p999_ns: parse_field(fields[ROUND_TRIP_P999_FIELD], "round_trip_p999_ns")?,
        throughput_messages_per_second: parse_finite_field(
            fields[THROUGHPUT_FIELD],
            "throughput_messages_per_second",
        )?,
        payload_throughput_gib_per_second: parse_finite_field(
            fields[PAYLOAD_THROUGHPUT_FIELD],
            "payload_throughput_gib_per_second",
        )?,
        allocations_per_message: parse_finite_field(
            fields[ALLOCATIONS_FIELD],
            "allocations_per_message",
        )?,
        syscalls_per_message: parse_finite_field(fields[SYSCALLS_FIELD], "syscalls_per_message")?,
        warning_count: parse_field(fields[WARNING_COUNT_FIELD], "warning_count")?,
        skipped_messages: parse_field(fields[SKIPPED_MESSAGES_FIELD], "skipped_messages")?,
        backpressure_pinned_slot: parse_field(
            fields[BACKPRESSURE_PINNED_SLOT_FIELD],
            "backpressure_pinned_slot",
        )?,
    })
}

fn parse_key_field(value: &str, _field_name: &'static str) -> Result<String, Warning> {
    if value.is_empty() || value == WILDCARD_KEY {
        return Err(gate_warning("parse_baseline_entry::key"));
    }
    Ok(value.to_owned())
}

fn parse_payload_size_field(value: &str) -> Result<usize, Warning> {
    let payload_size = parse_field::<usize>(value, "payload_size")?;
    if PAYLOAD_SIZES.contains(&payload_size) {
        return Ok(payload_size);
    }
    Err(gate_warning("parse_baseline_entry::payload_size"))
}

fn parse_field<T>(value: &str, _field_name: &'static str) -> Result<T, Warning>
where
    T: core::str::FromStr,
{
    value
        .parse::<T>()
        .map_err(|_| gate_warning("parse_baseline_entry::parse"))
}

fn parse_finite_field(value: &str, field_name: &'static str) -> Result<f64, Warning> {
    let parsed = parse_field::<f64>(value, field_name)?;
    if !parsed.is_finite() || parsed.is_sign_negative() {
        return Err(gate_warning("parse_baseline_entry::finite"));
    }
    Ok(parsed)
}

fn enforce_latency_gate(actual: u64, baseline: u64, _metric: &'static str) -> Result<(), Warning> {
    let threshold = baseline.saturating_mul(LATENCY_GATE_NUMERATOR) / LATENCY_GATE_DENOMINATOR;
    if actual > threshold {
        return Err(gate_warning("enforce_regression_gates::latency"));
    }
    Ok(())
}

fn enforce_throughput_gate(actual: f64, baseline: f64) -> Result<(), Warning> {
    if actual < baseline * THROUGHPUT_GATE_FACTOR {
        return Err(gate_warning("enforce_regression_gates::throughput"));
    }
    Ok(())
}

fn enforce_payload_throughput_gate(actual: f64, baseline: f64) -> Result<(), Warning> {
    if baseline != 0.0 && actual < baseline * THROUGHPUT_GATE_FACTOR {
        return Err(gate_warning("enforce_regression_gates::payload_throughput"));
    }
    Ok(())
}

fn enforce_float_ceiling(
    actual: f64,
    baseline: f64,
    operation: &'static str,
) -> Result<(), Warning> {
    if actual > baseline {
        return Err(gate_warning(operation));
    }
    Ok(())
}

fn enforce_counter_ceiling(
    actual: u64,
    baseline: u64,
    operation: &'static str,
) -> Result<(), Warning> {
    if actual > baseline {
        return Err(gate_warning(operation));
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
    publisher_count: u8,
    subscriber_count: u8,
    measured_messages: u64,
    transport_latency_p50_ns: u64,
    transport_latency_p90_ns: u64,
    transport_latency_p99_ns: u64,
    transport_latency_p999_ns: u64,
    transport_latency_p9999_ns: u64,
    end_to_end_latency_p50_ns: u64,
    end_to_end_latency_p90_ns: u64,
    end_to_end_latency_p99_ns: u64,
    end_to_end_latency_p999_ns: u64,
    end_to_end_latency_p9999_ns: u64,
    round_trip_p50_ns: u64,
    round_trip_p90_ns: u64,
    round_trip_p99_ns: u64,
    round_trip_p999_ns: u64,
    throughput_messages_per_second: f64,
    payload_throughput_gib_per_second: f64,
    allocations_per_message: f64,
    syscalls_per_message: f64,
    warning_count: u64,
    skipped_messages: u64,
    backpressure_pinned_slot: u64,
}

impl BaselineEntry {
    fn matches(&self, result: &BenchmarkResult) -> bool {
        self.scenario == result.scenario
            && self.transport == result.transport.as_str()
            && self.wait_mode == result.wait_mode.as_str()
            && self.payload_size == result.payload_size
            && self.publisher_count == result.publisher_count
            && self.subscriber_count == result.subscriber_count
            && self.measured_messages == result.measured_messages
    }

    fn has_same_key(&self, other: &Self) -> bool {
        self.scenario == other.scenario
            && self.transport == other.transport
            && self.wait_mode == other.wait_mode
            && self.payload_size == other.payload_size
            && self.publisher_count == other.publisher_count
            && self.subscriber_count == other.subscriber_count
            && self.measured_messages == other.measured_messages
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::transport_kind::TransportKind;
    use crate::wait_mode::WaitMode;

    use super::*;

    const BASELINE_NAME_PREFIX: &str = "rbus-baseline-test";
    const TEST_SCENARIO: &str = "four-publishers-eight-subscribers";
    const ZERO_PAYLOAD_SIZE: usize = 0;
    const TEST_PAYLOAD_SIZE: usize = 64;
    const TEST_THROUGHPUT: f64 = 1_000.0;

    #[test]
    fn full_baseline_entry_allows_matching_result() {
        let path = write_baseline(
            "full",
            &baseline_line(
                TEST_SCENARIO,
                "rbus-loaned-metadata-only",
                TEST_PAYLOAD_SIZE,
                100,
                0,
            ),
        );
        let result = benchmark_result(100);
        enforce_regression_gates(&path, &[result]).expect("baseline should pass");
        remove_baseline(path);
    }

    #[test]
    fn baseline_gate_rejects_zero_payload_key() {
        let path = write_baseline(
            "zero-payload",
            &baseline_line(
                TEST_SCENARIO,
                "rbus-loaned-metadata-only",
                ZERO_PAYLOAD_SIZE,
                100,
                0,
            ),
        );
        let result = benchmark_result(100);
        assert!(enforce_regression_gates(&path, &[result]).is_err());
        remove_baseline(path);
    }

    #[test]
    fn baseline_gate_rejects_missing_key() {
        let path = write_baseline(
            "missing",
            &baseline_line(TEST_SCENARIO, "unix-socket", TEST_PAYLOAD_SIZE, 100, 0),
        );
        let result = benchmark_result(100);
        assert!(enforce_regression_gates(&path, &[result]).is_err());
        remove_baseline(path);
    }

    #[test]
    fn baseline_gate_rejects_latency_regression() {
        let path = write_baseline(
            "latency",
            &baseline_line(
                TEST_SCENARIO,
                "rbus-loaned-metadata-only",
                TEST_PAYLOAD_SIZE,
                10,
                0,
            ),
        );
        let result = benchmark_result(100);
        assert!(enforce_regression_gates(&path, &[result]).is_err());
        remove_baseline(path);
    }

    #[test]
    fn baseline_gate_rejects_warning_regression() {
        let path = write_baseline(
            "warning",
            &baseline_line(
                TEST_SCENARIO,
                "rbus-loaned-metadata-only",
                TEST_PAYLOAD_SIZE,
                100,
                0,
            ),
        );
        let mut result = benchmark_result(100);
        result.warning_count = 1;
        assert!(enforce_regression_gates(&path, &[result]).is_err());
        remove_baseline(path);
    }

    fn benchmark_result(latency_ns: u64) -> BenchmarkResult {
        BenchmarkResult {
            scenario: TEST_SCENARIO,
            transport: TransportKind::RbusLoanedMetadataOnly,
            wait_mode: WaitMode::BusyPoll,
            payload_size: TEST_PAYLOAD_SIZE,
            publisher_count: 1,
            subscriber_count: 1,
            measured_messages: 1,
            transport_latency_p50_ns: latency_ns,
            transport_latency_p90_ns: latency_ns,
            transport_latency_p99_ns: latency_ns,
            transport_latency_p999_ns: latency_ns,
            transport_latency_p9999_ns: latency_ns,
            end_to_end_latency_p50_ns: latency_ns,
            end_to_end_latency_p90_ns: latency_ns,
            end_to_end_latency_p99_ns: latency_ns,
            end_to_end_latency_p999_ns: latency_ns,
            end_to_end_latency_p9999_ns: latency_ns,
            round_trip_p50_ns: 0,
            round_trip_p90_ns: 0,
            round_trip_p99_ns: 0,
            round_trip_p999_ns: 0,
            throughput_messages_per_second: TEST_THROUGHPUT,
            payload_throughput_gib_per_second: 0.0,
            cpu_utilization_per_mille: 0,
            rss_bytes: 0,
            pss_bytes: 0,
            context_switches: 0,
            syscalls_per_message: 0.0,
            allocations_per_message: 0.0,
            warning_count: 0,
            skipped_messages: 0,
            backpressure_pinned_slot: 0,
            startup_latency_ns: 0,
            attach_latency_ns: 0,
            orchestration_wall_time_ns: 0,
        }
    }

    fn baseline_line(
        scenario: &str,
        transport: &str,
        payload_size: usize,
        latency_ns: u64,
        warning_count: u64,
    ) -> String {
        format!(
            concat!(
                "ok,{},{},busy-poll,{},1,1,1,",
                "{},{},{},{},{},{},{},{},{},{},",
                "0,0,0,0,{:.6},0.000000000,0,0,0,0,",
                "0.000000,0.000000,{},0,0,0,0,0"
            ),
            scenario,
            transport,
            payload_size,
            latency_ns,
            latency_ns,
            latency_ns,
            latency_ns,
            latency_ns,
            latency_ns,
            latency_ns,
            latency_ns,
            latency_ns,
            latency_ns,
            TEST_THROUGHPUT,
            warning_count
        )
    }

    fn write_baseline(name: &str, line: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "{BASELINE_NAME_PREFIX}-{}-{name}.csv",
            std::process::id()
        ));
        std::fs::write(&path, format!("{CSV_HEADER}\n{line}\n")).expect("baseline should write");
        path
    }

    fn remove_baseline(path: PathBuf) {
        std::fs::remove_file(path).expect("baseline should remove");
    }
}
