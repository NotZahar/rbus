use std::process::Command;

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
}
