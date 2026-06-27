use std::process::Command;

#[test]
fn cli_list_reports_unavailable_daemon_without_fallback() {
    let binary_path = env!("CARGO_BIN_EXE_rbus-cli");
    let output = Command::new(binary_path)
        .args(["list", "--domain", "cli-no-daemon-list", "--json"])
        .output()
        .expect("rbus-cli should run successfully");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("value should be utf8");
    assert!(stderr.contains("DaemonUnavailable"));
}

#[test]
fn cli_inspect_accepts_topic_argument_then_requires_daemon() {
    let binary_path = env!("CARGO_BIN_EXE_rbus-cli");
    let output = Command::new(binary_path)
        .args([
            "inspect",
            "--topic",
            "/control/cmd",
            "--domain",
            "cli-no-daemon-inspect",
            "--json",
        ])
        .output()
        .expect("rbus-cli should run successfully");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("value should be utf8");
    assert!(stderr.contains("DaemonUnavailable"));
}

#[test]
fn cli_inspect_rejects_missing_topic_value() {
    let binary_path = env!("CARGO_BIN_EXE_rbus-cli");
    let output = Command::new(binary_path)
        .args(["inspect", "--topic"])
        .output()
        .expect("rbus-cli should run successfully");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("value should be utf8");
    assert!(stderr.contains("InvalidConfig"));
}
