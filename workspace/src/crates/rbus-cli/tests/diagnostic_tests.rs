use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use rbus::{Bus, Publisher, Subscriber, TopicConfig, WarningCode};
use rbus_core::constants::{
    CONTROL_PROTOCOL_VERSION, MAX_SEGMENT_SIZE, RBUS_API_VERSION, STREAM_SEGMENT_ABI_VERSION,
    TOPIC_WAIT_ABI_VERSION,
};
use rbus_ipc::control::client::request_control_frame;
use rbus_ipc::control::opcode::Opcode;
use rbus_ipc::unix_socket::default_socket_path;
use serde_json::Value;

const CLI_COMMAND_NAMES: [&str; 7] = [
    "list",
    "stats",
    "warnings",
    "inspect",
    "cleanup",
    "doctor",
    "bench-smoke",
];
const TEST_TOPIC: &str = "/diagnostics/topic";
const TEST_MAX_MESSAGE_SIZE: usize = 128;
const TEST_DEPTH: usize = 8;
const OVERSIZED_MESSAGE_SIZE: usize = TEST_MAX_MESSAGE_SIZE + 1;
const DAEMON_WAIT_ATTEMPTS: usize = 100;
const DAEMON_WAIT_STEP: Duration = Duration::from_millis(10);
const RECEIVE_TIMEOUT: Duration = Duration::from_secs(1);
const WARNING_EXIT_CODE: i32 = 2;
const EXPECTED_DIRECTORY_MODE: u64 = 0o700;

static NEXT_DOMAIN_ID: AtomicU64 = AtomicU64::new(0);

struct DaemonProcess {
    child: Child,
    domain: String,
}

impl DaemonProcess {
    fn start(label: &str) -> Self {
        let domain = unique_domain(label);
        let socket_path = default_socket_path(&domain);
        let _ = std::fs::remove_file(&socket_path);
        let child = Command::new(rbusd_binary_path())
            .args(["--domain", &domain])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("rbusd should run successfully");
        wait_until_daemon_accepts(&domain);
        Self { child, domain }
    }
}

impl Drop for DaemonProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(default_socket_path(&self.domain));
    }
}

#[test]
fn every_cli_command_prints_command_help() {
    for command_name in CLI_COMMAND_NAMES {
        let stdout = assert_success(run_cli([command_name, "--help"]));
        let expected_usage = format!("rbus-cli {command_name}");
        assert!(
            stdout.contains(&expected_usage),
            "help for {command_name} should contain {expected_usage}: {stdout}"
        );
    }
}

#[test]
fn every_cli_command_succeeds_with_running_daemon() {
    let daemon = DaemonProcess::start("cli-success");
    let (_publisher, _subscriber) = seed_daemon_data(&daemon.domain);

    for command_name in CLI_COMMAND_NAMES {
        let output = run_cli(command_arguments(command_name, &daemon.domain, false));
        assert_success_contains(output, expected_success_fragments(command_name));
    }
}

#[test]
fn every_cli_command_warning_path_is_fatal() {
    for command_name in CLI_COMMAND_NAMES {
        let output = run_cli(warning_arguments(command_name));
        let expected_warning = expected_warning_code(command_name);

        assert_eq!(output.status.code(), Some(WARNING_EXIT_CODE));
        let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
        assert!(
            stderr.contains(expected_warning),
            "stderr for {command_name} should contain {expected_warning}: {stderr}"
        );
        assert!(
            stderr.contains("stacktrace:"),
            "stderr for {command_name} should contain stacktrace: {stderr}"
        );
    }
}

#[test]
fn every_cli_command_json_output_is_valid() {
    let daemon = DaemonProcess::start("cli-json");
    let (_publisher, _subscriber) = seed_daemon_data(&daemon.domain);

    for command_name in CLI_COMMAND_NAMES {
        let output = run_cli(command_arguments(command_name, &daemon.domain, true));
        let value = assert_valid_json(output);

        assert!(
            value.is_object(),
            "JSON for {command_name} should be an object"
        );
        assert_eq!(
            value.get("domain").and_then(Value::as_str),
            Some(daemon.domain.as_str())
        );
        if command_name == "inspect" {
            assert_eq!(value.get("topic").and_then(Value::as_str), Some(TEST_TOPIC));
        }
        if command_name == "doctor" {
            assert_eq!(
                value
                    .get("runtime_directory_exists")
                    .and_then(Value::as_bool),
                Some(true)
            );
            assert_eq!(
                value.get("runtime_directory_mode").and_then(Value::as_u64),
                Some(EXPECTED_DIRECTORY_MODE)
            );
            assert_eq!(
                value.get("socket_directory_mode").and_then(Value::as_u64),
                Some(EXPECTED_DIRECTORY_MODE)
            );
            assert_eq!(
                value.get("api_version").and_then(Value::as_str),
                Some(RBUS_API_VERSION)
            );
            assert_eq!(
                value
                    .get("control_protocol_version")
                    .and_then(Value::as_u64),
                Some(u64::from(CONTROL_PROTOCOL_VERSION))
            );
            assert_eq!(
                value
                    .get("stream_segment_abi_version")
                    .and_then(Value::as_u64),
                Some(u64::from(STREAM_SEGMENT_ABI_VERSION))
            );
            assert_eq!(
                value.get("topic_wait_abi_version").and_then(Value::as_u64),
                Some(u64::from(TOPIC_WAIT_ABI_VERSION))
            );
            assert_eq!(value.get("mmap").and_then(Value::as_bool), Some(true));
            assert_eq!(value.get("scm_rights").and_then(Value::as_bool), Some(true));
            assert_eq!(
                value.get("max_segment_size").and_then(Value::as_u64),
                Some(MAX_SEGMENT_SIZE as u64)
            );
        }
        if command_name == "bench-smoke" {
            let bench_smoke = value
                .get("bench_smoke")
                .expect("bench-smoke JSON should contain report");
            assert_eq!(
                bench_smoke.get("status").and_then(Value::as_str),
                Some("ok")
            );
            assert_eq!(
                bench_smoke.get("warning_count").and_then(Value::as_u64),
                Some(0)
            );
            assert!(
                bench_smoke
                    .get("blocking_wait_wake_latency_ns")
                    .and_then(Value::as_u64)
                    .is_some(),
                "bench-smoke JSON should contain measured latency"
            );
        }
    }
}

#[test]
fn bench_smoke_runs_real_benchmark_report() {
    let output = run_cli(["bench-smoke"]);
    let value = assert_valid_json(output);

    assert_eq!(value.get("status").and_then(Value::as_str), Some("ok"));
    assert_eq!(value.get("warning_count").and_then(Value::as_u64), Some(0));
    assert!(
        value
            .get("blocking_wait_wake_latency_ns")
            .and_then(Value::as_u64)
            .is_some(),
        "bench-smoke should print rbus-bench smoke report"
    );
    assert!(
        value.get("bench_smoke").is_none(),
        "bench-smoke without --json should not print the old CLI stub shape"
    );
}

fn run_cli<I, S>(arguments: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(env!("CARGO_BIN_EXE_rbus-cli"))
        .args(arguments)
        .output()
        .expect("rbus-cli should run successfully")
}

fn assert_success_contains(output: Output, expected_fragments: &[&str]) {
    let stdout = assert_success(output);
    for expected in expected_fragments {
        assert!(
            stdout.contains(expected),
            "stdout should contain {expected}: {stdout}"
        );
    }
}

fn assert_success(output: Output) -> String {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("value should be utf8")
}

fn assert_valid_json(output: Output) -> Value {
    let stdout = assert_success(output);
    serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON")
}

fn seed_daemon_data(domain: &str) -> (Publisher, Subscriber) {
    let bus = Bus::connect(domain).expect("operation should succeed");
    let mut publisher = bus
        .publisher(
            TEST_TOPIC,
            TopicConfig::new(TEST_MAX_MESSAGE_SIZE, TEST_DEPTH),
        )
        .expect("publisher operation should succeed");
    let subscriber = bus
        .subscriber(TEST_TOPIC)
        .expect("subscriber operation should succeed");

    publisher
        .publish_bytes(b"hello")
        .expect("publish should succeed");
    let message = subscriber
        .recv_timeout(RECEIVE_TIMEOUT)
        .expect("operation should return warning")
        .expect("message should be available");
    assert_eq!(message.as_slice(), b"hello");
    drop(message);

    let warning = match publisher.loan(OVERSIZED_MESSAGE_SIZE) {
        Ok(_) => panic!("operation should return warning"),
        Err(warning) => warning,
    };
    assert_eq!(warning.code(), WarningCode::MessageTooLarge);

    (publisher, subscriber)
}

fn command_arguments(command_name: &str, domain: &str, json: bool) -> Vec<String> {
    let mut arguments = Vec::new();
    arguments.push(command_name.to_string());
    match command_name {
        "inspect" => {
            arguments.push(String::from("--topic"));
            arguments.push(String::from(TEST_TOPIC));
        }
        "list" | "stats" | "warnings" | "cleanup" | "doctor" | "bench-smoke" => {}
        _ => panic!("unknown CLI command {command_name}"),
    }
    arguments.push(String::from("--domain"));
    arguments.push(domain.to_string());
    if json {
        arguments.push(String::from("--json"));
    }
    arguments
}

fn warning_arguments(command_name: &str) -> Vec<String> {
    if command_name == "bench-smoke" {
        return vec![
            String::from("bench-smoke"),
            String::from("--unexpected-option"),
        ];
    }
    let domain_label = format!("missing-{command_name}");
    let domain = unique_domain(&domain_label);
    command_arguments(command_name, &domain, true)
}

fn expected_success_fragments(command_name: &str) -> &'static [&'static str] {
    match command_name {
        "list" | "inspect" => &[
            "topics=1",
            "publishers=1",
            "subscribers=1",
            "streams=1",
            "stream stream_id=1",
        ],
        "stats" => &[
            "published=1",
            "received=1",
            "warnings=1",
            "connected_streams=1",
        ],
        "warnings" => &["warnings_total=1", "samples=1"],
        "cleanup" => &["cleanup removed=0", "socket_paths_removed=0"],
        "doctor" => &[
            "runtime_directory_exists=true",
            "runtime_directory_mode=700",
            "socket_exists=true",
            "socket_directory_mode=700",
            "memfd=true",
            "mmap=true",
            "eventfd=true",
            "futex=true",
            "scm_rights=true",
            "max_segment_size=",
            "warnings_total=1",
        ],
        "bench-smoke" => &[
            "\"status\":\"ok\"",
            "\"warning_count\":0",
            "\"blocking_wait_wake_latency_ns\":",
        ],
        _ => panic!("unknown CLI command {command_name}"),
    }
}

fn expected_warning_code(command_name: &str) -> &'static str {
    match command_name {
        "bench-smoke" => "InvalidConfig",
        "list" | "stats" | "warnings" | "inspect" | "cleanup" | "doctor" => "DaemonUnavailable",
        _ => panic!("unknown CLI command {command_name}"),
    }
}

fn wait_until_daemon_accepts(domain: &str) {
    let socket_path = default_socket_path(domain);
    for _attempt in 0..DAEMON_WAIT_ATTEMPTS {
        if request_control_frame(&socket_path, Opcode::Hello, 0, &[], 0).is_ok() {
            return;
        }
        thread::sleep(DAEMON_WAIT_STEP);
    }
    panic!("rbusd should accept HELLO");
}

fn unique_domain(label: &str) -> String {
    let domain_id = NEXT_DOMAIN_ID.fetch_add(1, Ordering::Relaxed);
    format!("rbus-cli-{label}-{}-{domain_id}", std::process::id())
}

fn rbusd_binary_path() -> PathBuf {
    let current_executable =
        std::env::current_exe().expect("test executable path should be readable");
    let debug_directory = current_executable
        .parent()
        .and_then(|directory| directory.parent())
        .expect("test executable should be in target/debug/deps");
    debug_directory.join("rbusd")
}
