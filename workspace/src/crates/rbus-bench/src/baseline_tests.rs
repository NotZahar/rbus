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
        load_mode: crate::load_mode::LoadMode::Saturated,
        rate_hz: 0.0,
        rate_scope: crate::load_mode::RateScope::PerPublisher,
        duration_ms: 0,
        configured_messages_per_publisher: 1,
        actual_messages_per_publisher: 1,
        measured_messages: 1,
        publish_deadline_miss_count: 0,
        publish_lateness_p50_ns: 0,
        publish_lateness_p90_ns: 0,
        publish_lateness_p99_ns: 0,
        publish_lateness_max_ns: 0,
        inter_publish_gap_p50_ns: 0,
        inter_publish_gap_p99_ns: 0,
        inter_receive_gap_p50_ns: 0,
        inter_receive_gap_p99_ns: 0,
        backlog_metric_available: true,
        ring_backlog_p50_messages: 0,
        ring_backlog_p90_messages: 0,
        ring_backlog_p99_messages: 0,
        ring_backlog_max_messages: 0,
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
        unique_publish_messages_per_second: TEST_THROUGHPUT,
        delivered_messages_per_second: TEST_THROUGHPUT,
        actual_payload_write_gib_per_second: 0.0,
        effective_payload_delivery_gib_per_second: 0.0,
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
            "ok,{},{},busy-poll,{},1,1,saturated,0.000000000,per-publisher,0,1,1,1,",
            "0,0,0,0,0,0,0,0,0,true,0,0,0,0,",
            "{},{},{},{},{},{},{},{},{},{},",
            "0,0,0,0,{:.6},0.000000000,{:.6},{:.6},0.000000000,0.000000000,0,0,0,0,",
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
        TEST_THROUGHPUT,
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
