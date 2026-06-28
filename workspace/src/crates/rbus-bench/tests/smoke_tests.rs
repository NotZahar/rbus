use std::collections::BTreeSet;
use std::process::Command;

const BASELINE_FIELD_COUNT: usize = 30;
const BASELINE_LATENCY_P50_FIELD: usize = 8;
const BASELINE_LATENCY_P99_FIELD: usize = 10;
const BASELINE_PAYLOAD_SIZE_FIELD: usize = 4;
const BASELINE_PUBLISHER_COUNT_FIELD: usize = 5;
const BASELINE_SCENARIO_FIELD: usize = 1;
const BASELINE_SUBSCRIBER_COUNT_FIELD: usize = 6;
const BASELINE_THROUGHPUT_FIELD: usize = 17;
const BASELINE_TRANSPORT_FIELD: usize = 2;
const BASELINE_WAIT_MODE_FIELD: usize = 3;
const BASELINE_WARNING_COUNT_FIELD: usize = 25;
const BASELINE_SYSCALLS_FIELD: usize = 23;
const BASELINE_MEASURED_MESSAGES_FIELD: usize = 7;
const CSV_MINIMUM_FIELD_COUNT: usize = 24;
const CSV_SYSCALLS_FIELD: usize = 23;
const CSV_TRANSPORT_FIELD: usize = 2;
const CONTAINER_SOURCE_ROOT_LITERAL: &str = concat!("/workspace", "/src");
const BASELINE_MESSAGES_PER_PUBLISHER: u64 = 32;
const BASELINE_RUNS_PER_SCENARIO: u64 = 5;
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
const BASELINE_WAIT_MODES: &[&str] = &["busy-poll", "recv", "recv-timeout"];
const ALL_TRANSPORTS: &[&str] = &[
    "rbus-loaned",
    "rbus-publish-bytes",
    "unix-socket",
    "pipe",
    "tcp-loopback",
    "posix-message-queue",
];
const RBUS_TRANSPORTS: &[&str] = &["rbus-loaned", "rbus-publish-bytes"];
const BASELINE_TOPOLOGIES: &[ExpectedTopology] = &[
    ExpectedTopology::new(
        "one-publisher-one-subscriber",
        1,
        1,
        ExpectedTransportSet::All,
    ),
    ExpectedTopology::new(
        "one-publisher-two-subscribers",
        1,
        2,
        ExpectedTransportSet::All,
    ),
    ExpectedTopology::new(
        "one-publisher-four-subscribers",
        1,
        4,
        ExpectedTransportSet::All,
    ),
    ExpectedTopology::new(
        "two-publishers-one-subscriber",
        2,
        1,
        ExpectedTransportSet::All,
    ),
    ExpectedTopology::new(
        "four-publishers-one-subscriber",
        4,
        1,
        ExpectedTransportSet::All,
    ),
    ExpectedTopology::new(
        "four-publishers-four-subscribers",
        4,
        4,
        ExpectedTransportSet::All,
    ),
    ExpectedTopology::new("ping-pong-round-trip", 1, 1, ExpectedTransportSet::All),
    ExpectedTopology::new(
        "slow-subscriber-gap-detection",
        1,
        1,
        ExpectedTransportSet::All,
    ),
    ExpectedTopology::new(
        "crash-subscriber-message-ref",
        1,
        1,
        ExpectedTransportSet::RbusOnly,
    ),
    ExpectedTopology::new(
        "crash-publisher-stream-lifetime",
        1,
        1,
        ExpectedTransportSet::RbusOnly,
    ),
];

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
    assert!(stdout.contains("ok,one-publisher-one-subscriber,rbus-loaned,busy-poll,64"));
    assert!(stdout.contains("ok,one-publisher-one-subscriber,rbus-publish-bytes,busy-poll,64"));
    assert!(stdout.contains("ok,one-publisher-one-subscriber,unix-socket,busy-poll,64"));
    assert!(stdout.contains("ok,one-publisher-one-subscriber,pipe,busy-poll,64"));
    assert!(stdout.contains("ok,one-publisher-one-subscriber,tcp-loopback,busy-poll,64"));
    assert!(stdout.contains("ok,one-publisher-one-subscriber,posix-message-queue,busy-poll,64"));
    assert!(syscalls_per_message(&stdout, UNIX_SOCKET_TRANSPORT) > 0.0);
}

#[test]
fn bench_suite_baseline_uses_production_thresholds() {
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
        let latency_p50_ns = fields[BASELINE_LATENCY_P50_FIELD]
            .parse::<u64>()
            .expect("p50 should be numeric");
        let latency_p99_ns = fields[BASELINE_LATENCY_P99_FIELD]
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
                        measured_messages: topology.measured_messages(),
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
    transport_set: ExpectedTransportSet,
}

impl ExpectedTopology {
    const fn new(
        scenario: &'static str,
        publisher_count: u8,
        subscriber_count: u8,
        transport_set: ExpectedTransportSet,
    ) -> Self {
        Self {
            scenario,
            publisher_count,
            subscriber_count,
            transport_set,
        }
    }

    fn transports(self) -> &'static [&'static str] {
        match self.transport_set {
            ExpectedTransportSet::All => ALL_TRANSPORTS,
            ExpectedTransportSet::RbusOnly => RBUS_TRANSPORTS,
        }
    }

    fn measured_messages(self) -> u64 {
        match self.transport_set {
            ExpectedTransportSet::All => {
                u64::from(self.publisher_count)
                    * u64::from(self.subscriber_count)
                    * BASELINE_MESSAGES_PER_PUBLISHER
                    * BASELINE_RUNS_PER_SCENARIO
            }
            ExpectedTransportSet::RbusOnly => {
                BASELINE_MESSAGES_PER_PUBLISHER * BASELINE_RUNS_PER_SCENARIO
            }
        }
    }
}

#[derive(Clone, Copy)]
enum ExpectedTransportSet {
    All,
    RbusOnly,
}
