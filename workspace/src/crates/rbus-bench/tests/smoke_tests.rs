use std::process::Command;

const BASELINE_FIELD_COUNT: usize = 9;
const BASELINE_LATENCY_P50_FIELD: usize = 4;
const BASELINE_LATENCY_P99_FIELD: usize = 5;
const BASELINE_THROUGHPUT_FIELD: usize = 6;
const CSV_MINIMUM_FIELD_COUNT: usize = 24;
const CSV_SYSCALLS_FIELD: usize = 23;
const CSV_TRANSPORT_FIELD: usize = 2;
const MAXIMUM_PORTABLE_P50_NS: u64 = 50_000_000;
const MAXIMUM_PORTABLE_P99_NS: u64 = 250_000_000;
const MINIMUM_PORTABLE_THROUGHPUT: f64 = 100.0;
const UNIX_SOCKET_TRANSPORT: &str = "unix-socket";

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
        .join("../../benches/baselines/rbus_suite_baseline.csv");
    let contents = std::fs::read_to_string(baseline_path).expect("baseline should be readable");
    for line in contents.lines().skip(1) {
        let fields: Vec<&str> = line.split(',').collect();
        assert_eq!(fields.len(), BASELINE_FIELD_COUNT);
        let latency_p50_ns = fields[BASELINE_LATENCY_P50_FIELD]
            .parse::<u64>()
            .expect("p50 should be numeric");
        let latency_p99_ns = fields[BASELINE_LATENCY_P99_FIELD]
            .parse::<u64>()
            .expect("p99 should be numeric");
        let throughput = fields[BASELINE_THROUGHPUT_FIELD]
            .parse::<f64>()
            .expect("throughput should be numeric");
        assert!(latency_p50_ns <= MAXIMUM_PORTABLE_P50_NS);
        assert!(latency_p99_ns <= MAXIMUM_PORTABLE_P99_NS);
        assert!(throughput >= MINIMUM_PORTABLE_THROUGHPUT);
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
