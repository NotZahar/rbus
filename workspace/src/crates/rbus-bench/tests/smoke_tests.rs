use std::collections::BTreeSet;
use std::process::Command;

const BASELINE_FIELD_COUNT: usize = 60;
const BASELINE_TRANSPORT_LATENCY_P50_FIELD: usize = 28;
const BASELINE_TRANSPORT_LATENCY_P99_FIELD: usize = 30;
const BASELINE_PAYLOAD_SIZE_FIELD: usize = 4;
const BASELINE_PUBLISHER_COUNT_FIELD: usize = 5;
const BASELINE_SCENARIO_FIELD: usize = 1;
const BASELINE_SUBSCRIBER_COUNT_FIELD: usize = 6;
const BASELINE_THROUGHPUT_FIELD: usize = 42;
const BASELINE_TRANSPORT_FIELD: usize = 2;
const BASELINE_WAIT_MODE_FIELD: usize = 3;
const BASELINE_WARNING_COUNT_FIELD: usize = 54;
const BASELINE_SYSCALLS_FIELD: usize = 52;
const BASELINE_MEASURED_MESSAGES_FIELD: usize = 13;
const CSV_MINIMUM_FIELD_COUNT: usize = 53;
const CSV_SYSCALLS_FIELD: usize = 52;
const CSV_TRANSPORT_FIELD: usize = 2;
const CONTAINER_SOURCE_ROOT_LITERAL: &str = concat!("/workspace", "/src");
const BASELINE_MESSAGES_PER_PUBLISHER: u64 = 100_000;
const BASELINE_RUNS_PER_SCENARIO: u64 = 1;
const MAX_PAYLOAD_BYTES_PER_STREAM: usize = 512 * 1024 * 1024;
const DEPTH_MINIMUM: usize = 8;
const POWER_OF_TWO_FACTOR: usize = 2;
const MAXIMUM_BASELINE_P50_NS: u64 = 250_000_000;
const MAXIMUM_BASELINE_P99_NS: u64 = 400_000_000;
const MAXIMUM_BASELINE_SYSCALLS_PER_MESSAGE: f64 = 150_000.0;
const MINIMUM_PORTABLE_THROUGHPUT: f64 = 1.0;
const UNIX_SOCKET_TRANSPORT: &str = "unix-socket";
const WILDCARD_KEY: &str = "*";
const BASELINE_FILE: &str = "rbus_suite_baseline_x86_64.csv";
const BASELINE_PAYLOAD_SIZES: &[usize] = &[
    64,
    256,
    1024,
    4 * 1024,
    16 * 1024,
    64 * 1024,
    256 * 1024,
    1024 * 1024,
    4 * 1024 * 1024,
    8 * 1024 * 1024,
];
const BASELINE_WAIT_MODES: &[&str] = &["busy-poll"];
const ALL_TRANSPORTS: &[&str] = &[
    "rbus-loaned-metadata-only",
    "rbus-loaned-fill",
    "rbus-publish-bytes",
    "unix-socket",
    "unix-socket-end-to-end",
];
const BASELINE_TOPOLOGIES: &[ExpectedTopology] = &[ExpectedTopology::new(
    "four-publishers-eight-subscribers",
    4,
    8,
)];

#[test]
fn bench_smoke_outputs_json_status() {
    let binary_path = env!("CARGO_BIN_EXE_rbus-bench");
    let output = Command::new(binary_path)
        .arg("--smoke")
        .output()
        .expect("rbus-bench should run successfully");
    assert!(output.status.success(), "operation should succeed");
    let stdout = String::from_utf8(output.stdout).expect("value should be utf8");
    assert!(stdout.contains("\"status\":\"ok\""));
}

#[test]
fn paced_smoke_runs_one_publisher_one_subscriber_at_30_hz() -> Result<(), Box<dyn std::error::Error>>
{
    let stdout = run_paced_smoke("rbus-loaned-metadata-only")?;
    assert!(stdout.contains("\"status\":\"ok\""));
    assert!(stdout.contains("\"scenario\":\"one-publisher-one-subscriber\""));
    assert!(stdout.contains("\"load_mode\":\"paced\""));
    assert!(stdout.contains("\"rate_hz\":30.000000000"));
    Ok(())
}

#[test]
fn paced_smoke_reports_zero_warnings_for_normal_case() -> Result<(), Box<dyn std::error::Error>> {
    let stdout = run_paced_smoke("rbus-loaned-metadata-only")?;
    assert!(stdout.contains("\"warning_count\":0"));
    assert!(stdout.contains("\"skipped_messages\":0"));
    assert!(stdout.contains("\"backpressure_pinned_slot\":0"));
    Ok(())
}

#[test]
fn paced_smoke_reports_zero_allocations_for_rbus_loaned_path()
-> Result<(), Box<dyn std::error::Error>> {
    let stdout = run_paced_smoke("rbus-loaned-fill")?;
    assert!(stdout.contains("\"allocations_per_message\":0.000000"));
    Ok(())
}

#[test]
fn paced_smoke_reports_zero_syscalls_for_rbus_busy_poll_hot_path()
-> Result<(), Box<dyn std::error::Error>> {
    let stdout = run_paced_smoke("rbus-loaned-metadata-only")?;
    assert!(stdout.contains("\"syscalls_per_message\":0.000000"));
    Ok(())
}

#[test]
fn paced_smoke_records_backlog_metrics() -> Result<(), Box<dyn std::error::Error>> {
    let stdout = run_paced_smoke("rbus-loaned-metadata-only")?;
    assert!(stdout.contains("\"backlog_metric_available\":true"));
    assert!(stdout.contains("\"ring_backlog_p99_messages\""));
    assert!(stdout.contains("\"ring_backlog_max_messages\""));
    Ok(())
}

#[test]
fn paced_smoke_fails_fast_on_invalid_rate() -> Result<(), Box<dyn std::error::Error>> {
    let binary_path = env!("CARGO_BIN_EXE_rbus-bench");
    let output = Command::new(binary_path)
        .args([
            "--smoke",
            "--load-mode",
            "paced",
            "--rate-hz",
            "60",
            "--duration-ms",
            "1000",
        ])
        .output()?;
    assert!(!output.status.success(), "operation should fail");
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("InvalidConfig"));
    assert!(stderr.contains("stacktrace"));
    Ok(())
}

#[test]
fn bench_rejects_unknown_hot_path_probe_operation() {
    let binary_path = env!("CARGO_BIN_EXE_rbus-bench");
    let output = Command::new(binary_path)
        .args(["--hot-path-syscall-probe", "unknown"])
        .output()
        .expect("rbus-bench should run successfully");
    assert!(!output.status.success(), "operation should succeed");
    let stderr = String::from_utf8(output.stderr).expect("value should be utf8");
    assert!(stderr.contains("InvalidConfig"));
    assert!(stderr.contains("stacktrace"));
}

fn run_paced_smoke(transport: &str) -> Result<String, Box<dyn std::error::Error>> {
    let binary_path = env!("CARGO_BIN_EXE_rbus-bench");
    let output = Command::new(binary_path)
        .args([
            "--smoke",
            "--load-mode",
            "paced",
            "--rate-hz",
            "30",
            "--duration-ms",
            "1000",
            "--transport",
            transport,
        ])
        .output()?;
    assert!(output.status.success(), "operation should succeed");
    Ok(String::from_utf8(output.stdout)?)
}

#[test]
fn bench_suite_matrix_smoke_outputs_csv() {
    let binary_path = env!("CARGO_BIN_EXE_rbus-bench");
    let output = Command::new(binary_path)
        .args([
            "--suite",
            "--matrix-smoke",
            "--messages",
            "1",
            "--format",
            "csv",
            "--no-gate",
        ])
        .output()
        .expect("rbus-bench suite should run successfully");
    assert!(output.status.success(), "operation should succeed");
    let stdout = String::from_utf8(output.stdout).expect("value should be utf8");
    assert!(stdout.contains("status,scenario,transport,wait_mode,payload_size"));
    assert!(
        stdout.contains(
            "ok,four-publishers-eight-subscribers,rbus-loaned-metadata-only,busy-poll,64"
        )
    );
    assert!(stdout.contains("ok,four-publishers-eight-subscribers,rbus-loaned-fill,busy-poll,64"));
    assert!(
        stdout.contains("ok,four-publishers-eight-subscribers,rbus-publish-bytes,busy-poll,64")
    );
    assert!(stdout.contains("ok,four-publishers-eight-subscribers,unix-socket,busy-poll,64"));
    assert!(
        stdout.contains("ok,four-publishers-eight-subscribers,unix-socket-end-to-end,busy-poll,64")
    );
    assert!(syscalls_per_message(&stdout, UNIX_SOCKET_TRANSPORT) > 0.0);
}

#[test]
fn bench_suite_accepts_explicit_recv_wait_mode() {
    let binary_path = env!("CARGO_BIN_EXE_rbus-bench");
    let output = Command::new(binary_path)
        .args([
            "--suite",
            "--matrix-smoke",
            "--messages",
            "1",
            "--format",
            "csv",
            "--no-gate",
            "--transport",
            "rbus-loaned-metadata-only",
            "--wait-mode",
            "recv",
        ])
        .output()
        .expect("rbus-bench suite should run successfully");
    assert!(output.status.success(), "operation should succeed");
    let stdout = String::from_utf8(output.stdout).expect("value should be utf8");
    assert!(
        stdout.contains("ok,four-publishers-eight-subscribers,rbus-loaned-metadata-only,recv,64")
    );
}

#[test]
fn bench_suite_csv_streams_rows_before_gate_failure() {
    let binary_path = env!("CARGO_BIN_EXE_rbus-bench");
    let missing_baseline = std::env::temp_dir().join(format!(
        "rbus-bench-missing-baseline-{}.csv",
        std::process::id()
    ));
    let output = Command::new(binary_path)
        .args([
            "--suite",
            "--matrix-smoke",
            "--messages",
            "1",
            "--format",
            "csv",
            "--baseline",
        ])
        .arg(missing_baseline)
        .output()
        .expect("rbus-bench suite should run successfully");
    assert!(!output.status.success(), "operation should fail");
    let stdout = String::from_utf8(output.stdout).expect("value should be utf8");
    let stderr = String::from_utf8(output.stderr).expect("value should be utf8");
    assert!(stdout.starts_with("status,scenario,transport,wait_mode,payload_size"));
    assert_eq!(stdout.lines().count(), ALL_TRANSPORTS.len() + 1);
    assert!(stderr.contains("stacktrace"));
}

#[test]
fn bench_suite_baseline_uses_raw_production_measurements() {
    let baseline_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../benches/baselines")
        .join(BASELINE_FILE);
    let contents = std::fs::read_to_string(baseline_path).expect("baseline should be readable");
    let mut keys = BTreeSet::new();
    for line in contents.lines().skip(1) {
        let fields: Vec<&str> = line.split(',').collect();
        assert_eq!(fields.len(), BASELINE_FIELD_COUNT);
        assert_ne!(fields[BASELINE_SCENARIO_FIELD], WILDCARD_KEY);
        assert_ne!(fields[BASELINE_TRANSPORT_FIELD], WILDCARD_KEY);
        assert_ne!(fields[BASELINE_WAIT_MODE_FIELD], WILDCARD_KEY);
        let payload_size = fields[BASELINE_PAYLOAD_SIZE_FIELD]
            .parse::<usize>()
            .expect("payload size should be numeric");
        let latency_p50_ns = fields[BASELINE_TRANSPORT_LATENCY_P50_FIELD]
            .parse::<u64>()
            .expect("p50 should be numeric");
        let latency_p99_ns = fields[BASELINE_TRANSPORT_LATENCY_P99_FIELD]
            .parse::<u64>()
            .expect("p99 should be numeric");
        let throughput = fields[BASELINE_THROUGHPUT_FIELD]
            .parse::<f64>()
            .expect("throughput should be numeric");
        let syscalls_per_message = fields[BASELINE_SYSCALLS_FIELD]
            .parse::<f64>()
            .expect("syscalls/message should be numeric");
        let warning_count = fields[BASELINE_WARNING_COUNT_FIELD]
            .parse::<u64>()
            .expect("warning count should be numeric");
        assert!(BASELINE_PAYLOAD_SIZES.contains(&payload_size));
        assert!(latency_p50_ns < MAXIMUM_BASELINE_P50_NS);
        assert!(latency_p99_ns < MAXIMUM_BASELINE_P99_NS);
        assert!(throughput > MINIMUM_PORTABLE_THROUGHPUT);
        assert!(syscalls_per_message < MAXIMUM_BASELINE_SYSCALLS_PER_MESSAGE);
        assert_eq!(warning_count, 0);
        assert!(keys.insert(BaselineKey::from_fields(&fields)));
    }
    assert_eq!(keys, expected_baseline_keys());
}

#[test]
fn benchmark_reference_perf_strace_scripts_exist() {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("workspace root should exist");
    for script_name in [
        "benchmark-reference.ts",
        "benchmark-perf.ts",
        "benchmark-strace.ts",
    ] {
        let script_path = workspace_root.join("scripts").join(script_name);
        let contents = std::fs::read_to_string(script_path).expect("script should be readable");
        assert!(contents.contains("rbus-bench"));
        assert!(contents.contains("ScriptError"));
    }
}

#[test]
fn benchmark_scripts_do_not_hardcode_container_source_root() {
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("workspace root should exist");
    for script_name in [
        "benchmark-reference.ts",
        "benchmark-perf.ts",
        "benchmark-strace.ts",
        "hot-path-syscall-smoke.ts",
        "workspace-paths.ts",
    ] {
        let script_path = workspace_root.join("scripts").join(script_name);
        let contents = std::fs::read_to_string(script_path).expect("script should be readable");
        assert!(!contents.contains(CONTAINER_SOURCE_ROOT_LITERAL));
    }
}

fn syscalls_per_message(output: &str, transport: &str) -> f64 {
    for line in output.lines().skip(1) {
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() >= CSV_MINIMUM_FIELD_COUNT && fields[CSV_TRANSPORT_FIELD] == transport {
            return fields[CSV_SYSCALLS_FIELD]
                .parse::<f64>()
                .expect("syscalls_per_message should be numeric");
        }
    }
    panic!("missing transport row: {transport}");
}

fn expected_baseline_keys() -> BTreeSet<BaselineKey> {
    let mut keys = BTreeSet::new();
    for topology in BASELINE_TOPOLOGIES {
        for payload_size in BASELINE_PAYLOAD_SIZES {
            for wait_mode in BASELINE_WAIT_MODES {
                for transport in topology.transports() {
                    keys.insert(BaselineKey {
                        scenario: topology.scenario.to_owned(),
                        transport: transport.to_string(),
                        wait_mode: (*wait_mode).to_owned(),
                        payload_size: *payload_size,
                        publisher_count: topology.publisher_count,
                        subscriber_count: topology.subscriber_count,
                        measured_messages: topology.measured_messages(*payload_size),
                    });
                }
            }
        }
    }
    keys
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct BaselineKey {
    scenario: String,
    transport: String,
    wait_mode: String,
    payload_size: usize,
    publisher_count: u8,
    subscriber_count: u8,
    measured_messages: u64,
}

impl BaselineKey {
    fn from_fields(fields: &[&str]) -> Self {
        Self {
            scenario: fields[BASELINE_SCENARIO_FIELD].to_owned(),
            transport: fields[BASELINE_TRANSPORT_FIELD].to_owned(),
            wait_mode: fields[BASELINE_WAIT_MODE_FIELD].to_owned(),
            payload_size: fields[BASELINE_PAYLOAD_SIZE_FIELD]
                .parse::<usize>()
                .expect("payload size should be numeric"),
            publisher_count: fields[BASELINE_PUBLISHER_COUNT_FIELD]
                .parse::<u8>()
                .expect("publisher count should be numeric"),
            subscriber_count: fields[BASELINE_SUBSCRIBER_COUNT_FIELD]
                .parse::<u8>()
                .expect("subscriber count should be numeric"),
            measured_messages: fields[BASELINE_MEASURED_MESSAGES_FIELD]
                .parse::<u64>()
                .expect("measured messages should be numeric"),
        }
    }
}

#[derive(Clone, Copy)]
struct ExpectedTopology {
    scenario: &'static str,
    publisher_count: u8,
    subscriber_count: u8,
}

impl ExpectedTopology {
    const fn new(scenario: &'static str, publisher_count: u8, subscriber_count: u8) -> Self {
        Self {
            scenario,
            publisher_count,
            subscriber_count,
        }
    }

    fn transports(self) -> &'static [&'static str] {
        ALL_TRANSPORTS
    }

    fn measured_messages(self, payload_size: usize) -> u64 {
        u64::from(self.publisher_count)
            * u64::from(self.subscriber_count)
            * messages_per_publisher(payload_size)
            * BASELINE_RUNS_PER_SCENARIO
    }
}

fn messages_per_publisher(payload_size: usize) -> u64 {
    BASELINE_MESSAGES_PER_PUBLISHER.min(byte_limited_depth(payload_size).saturating_sub(1) as u64)
}

fn byte_limited_depth(payload_size: usize) -> usize {
    floor_power_of_two((MAX_PAYLOAD_BYTES_PER_STREAM / payload_size).max(DEPTH_MINIMUM))
}

fn floor_power_of_two(value: usize) -> usize {
    let mut power = 1_usize;
    while power <= value / POWER_OF_TWO_FACTOR {
        power *= POWER_OF_TWO_FACTOR;
    }
    power
}
