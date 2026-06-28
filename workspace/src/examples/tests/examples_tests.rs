use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Output, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use rbus::{Bus, TopicConfig};
use rbus_ipc::control::client::request_control_frame;
use rbus_ipc::control::opcode::Opcode;
use rbus_ipc::unix_socket::default_socket_path;

const BASIC_PUB_BINARY: &str = env!("CARGO_BIN_EXE_basic_pub");
const BASIC_SUB_BINARY: &str = env!("CARGO_BIN_EXE_basic_sub");
const LOANED_PUB_BINARY: &str = env!("CARGO_BIN_EXE_loaned_pub");
const MULTI_SUB_BINARY: &str = env!("CARGO_BIN_EXE_multi_sub");
const PING_PONG_BINARY: &str = env!("CARGO_BIN_EXE_ping_pong");
const SLOW_SUBSCRIBER_BINARY: &str = env!("CARGO_BIN_EXE_slow_subscriber");
const RBUSD_BINARY_NAME: &str = "rbusd";
const MAX_MESSAGE_SIZE_TEXT: &str = "64";
const DEPTH_TEXT: &str = "8";
const HOLD_MILLISECONDS_TEXT: &str = "200";
const RECEIVE_TIMEOUT_MILLISECONDS_TEXT: &str = "1000";
const PAUSE_MILLISECONDS_TEXT: &str = "1";
const EXPECTED_MULTI_MESSAGES_TEXT: &str = "2";
const SLOW_MESSAGES_TEXT: &str = "3";
const TEST_MAX_MESSAGE_SIZE: usize = 64;
const TEST_DEPTH: usize = 8;
const EOF_BYTES_READ: usize = 0;
const DAEMON_WAIT_ATTEMPTS: usize = 100;
const DAEMON_WAIT_STEP: Duration = Duration::from_millis(10);
const RECEIVE_TIMEOUT: Duration = Duration::from_secs(1);

static NEXT_DOMAIN_ID: AtomicU64 = AtomicU64::new(0);
static RBUSD_BINARY_PATH: OnceLock<PathBuf> = OnceLock::new();

#[test]
fn basic_pub_publishes_payload_to_existing_subscriber() {
    let daemon = DaemonProcess::start("example-basic-pub");
    let topic = "/examples/basic-pub";
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let subscriber = bus
        .subscriber(topic)
        .expect("subscriber operation should succeed");

    let mut publisher_process = ChildProcess::spawn(
        BASIC_PUB_BINARY,
        &publisher_arguments(&daemon.domain, topic, "basic-payload"),
    );
    publisher_process.wait_until_line_starts_with("published sequence=");
    let message = subscriber
        .recv_timeout(RECEIVE_TIMEOUT)
        .expect("subscriber operation should succeed")
        .expect("message should be available");
    assert_eq!(message.as_slice(), b"basic-payload");
    publisher_process.wait_success("basic_pub");
}

#[test]
fn loaned_pub_publishes_payload_to_existing_subscriber() {
    let daemon = DaemonProcess::start("example-loaned-pub");
    let topic = "/examples/loaned-pub";
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let subscriber = bus
        .subscriber(topic)
        .expect("subscriber operation should succeed");

    let mut publisher_process = ChildProcess::spawn(
        LOANED_PUB_BINARY,
        &publisher_arguments(&daemon.domain, topic, "loaned-payload"),
    );
    publisher_process.wait_until_line_starts_with("loaned-published sequence=");
    let message = subscriber
        .recv_timeout(RECEIVE_TIMEOUT)
        .expect("subscriber operation should succeed")
        .expect("message should be available");
    assert_eq!(message.as_slice(), b"loaned-payload");
    publisher_process.wait_success("loaned_pub");
}

#[test]
fn basic_sub_receives_payload_from_parent_publisher() {
    let daemon = DaemonProcess::start("example-basic-sub");
    let topic = "/examples/basic-sub";
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let mut publisher = bus
        .publisher(topic, TopicConfig::new(TEST_MAX_MESSAGE_SIZE, TEST_DEPTH))
        .expect("publisher operation should succeed");
    let mut subscriber_process = ChildProcess::spawn(
        BASIC_SUB_BINARY,
        &[
            "--domain",
            &daemon.domain,
            "--topic",
            topic,
            "--timeout-ms",
            RECEIVE_TIMEOUT_MILLISECONDS_TEXT,
        ],
    );

    subscriber_process.wait_until_line("subscriber-ready");
    publisher
        .publish_bytes(b"sub-payload")
        .expect("publish should succeed");
    subscriber_process.wait_until_line_contains("payload=sub-payload");
    subscriber_process.wait_success("basic_sub");
}

#[test]
fn multi_sub_receives_two_payloads_from_parent_publisher() {
    let daemon = DaemonProcess::start("example-multi-sub");
    let topic = "/examples/multi-sub";
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let mut publisher = bus
        .publisher(topic, TopicConfig::new(TEST_MAX_MESSAGE_SIZE, TEST_DEPTH))
        .expect("publisher operation should succeed");
    let mut subscriber_process = ChildProcess::spawn(
        MULTI_SUB_BINARY,
        &[
            "--domain",
            &daemon.domain,
            "--topic",
            topic,
            "--expected-messages",
            EXPECTED_MULTI_MESSAGES_TEXT,
            "--timeout-ms",
            RECEIVE_TIMEOUT_MILLISECONDS_TEXT,
        ],
    );

    subscriber_process.wait_until_line("multi-subscriber-ready");
    publisher
        .publish_bytes(b"first")
        .expect("publish should succeed");
    subscriber_process.wait_until_line_contains("payload=first");
    publisher
        .publish_bytes(b"second")
        .expect("publish should succeed");
    subscriber_process.wait_until_line_contains("payload=second");
    subscriber_process.wait_success("multi_sub");
}

#[test]
fn ping_pong_client_and_server_exchange_payload() {
    let daemon = DaemonProcess::start("example-ping-pong");
    let ping_topic = "/examples/ping";
    let pong_topic = "/examples/pong";
    let mut server_process = ChildProcess::spawn(
        PING_PONG_BINARY,
        &ping_pong_arguments(&daemon.domain, "server", ping_topic, pong_topic, "unused"),
    );
    server_process.wait_until_line("server-ready");

    let output = run_process(
        PING_PONG_BINARY,
        &ping_pong_arguments(
            &daemon.domain,
            "client",
            ping_topic,
            pong_topic,
            "round-trip",
        ),
    );
    assert_success("ping_pong client", output);
    server_process.wait_until_line_contains("payload=round-trip");
    server_process.wait_success("ping_pong server");
}

#[test]
fn slow_subscriber_receives_generated_payloads() {
    let daemon = DaemonProcess::start("example-slow-subscriber");
    let output = run_process(
        SLOW_SUBSCRIBER_BINARY,
        &[
            "--domain",
            &daemon.domain,
            "--topic",
            "/examples/slow-subscriber",
            "--messages",
            SLOW_MESSAGES_TEXT,
            "--payload-prefix",
            "slow-",
            "--max-message-size",
            MAX_MESSAGE_SIZE_TEXT,
            "--depth",
            DEPTH_TEXT,
            "--pause-ms",
            PAUSE_MILLISECONDS_TEXT,
            "--timeout-ms",
            RECEIVE_TIMEOUT_MILLISECONDS_TEXT,
        ],
    );
    assert_success("slow_subscriber", output);
}

fn publisher_arguments<'a>(domain: &'a str, topic: &'a str, payload: &'a str) -> Vec<&'a str> {
    vec![
        "--domain",
        domain,
        "--topic",
        topic,
        "--payload",
        payload,
        "--max-message-size",
        MAX_MESSAGE_SIZE_TEXT,
        "--depth",
        DEPTH_TEXT,
        "--hold-ms",
        HOLD_MILLISECONDS_TEXT,
    ]
}

fn ping_pong_arguments<'a>(
    domain: &'a str,
    role: &'a str,
    ping_topic: &'a str,
    pong_topic: &'a str,
    payload: &'a str,
) -> Vec<&'a str> {
    vec![
        "--domain",
        domain,
        "--role",
        role,
        "--ping-topic",
        ping_topic,
        "--pong-topic",
        pong_topic,
        "--payload",
        payload,
        "--max-message-size",
        MAX_MESSAGE_SIZE_TEXT,
        "--depth",
        DEPTH_TEXT,
        "--timeout-ms",
        RECEIVE_TIMEOUT_MILLISECONDS_TEXT,
    ]
}

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

struct ChildProcess {
    child: Option<Child>,
    stdout: BufReader<ChildStdout>,
}

impl ChildProcess {
    fn spawn(binary_path: &str, arguments: &[&str]) -> Self {
        let mut child = Command::new(binary_path)
            .args(arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("example process should spawn");
        let stdout = child.stdout.take().expect("stdout should be piped");
        Self {
            child: Some(child),
            stdout: BufReader::new(stdout),
        }
    }

    fn wait_until_line(&mut self, expected_line: &str) {
        loop {
            let line = self.read_line();
            if line == expected_line {
                return;
            }
        }
    }

    fn wait_until_line_contains(&mut self, expected_fragment: &str) {
        loop {
            let line = self.read_line();
            if line.contains(expected_fragment) {
                return;
            }
        }
    }

    fn wait_until_line_starts_with(&mut self, expected_prefix: &str) {
        loop {
            let line = self.read_line();
            if line.starts_with(expected_prefix) {
                return;
            }
        }
    }

    fn read_line(&mut self) -> String {
        let mut line = String::new();
        let bytes_read = self
            .stdout
            .read_line(&mut line)
            .expect("stdout should contain expected output");
        if bytes_read == EOF_BYTES_READ {
            let output = self
                .child
                .take()
                .expect("child process should still be owned")
                .wait_with_output()
                .expect("child output should be readable");
            panic!(
                "example stdout closed early: status={} stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        line.trim_end().to_owned()
    }

    fn wait_success(&mut self, label: &str) {
        let output = self
            .child
            .take()
            .expect("child process should still be owned")
            .wait_with_output()
            .expect("child output should be readable");
        assert_success(label, output);
    }
}

impl Drop for ChildProcess {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn run_process(binary_path: &str, arguments: &[&str]) -> Output {
    Command::new(binary_path)
        .args(arguments)
        .output()
        .expect("example process should run")
}

fn assert_success(label: &str, output: Output) {
    assert!(
        output.status.success(),
        "{label} failed: status={} stdout={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn unique_domain(label: &str) -> String {
    let domain_id = NEXT_DOMAIN_ID.fetch_add(1, Ordering::Relaxed);
    format!("rbus-{label}-{}-{domain_id}", std::process::id())
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

fn rbusd_binary_path() -> PathBuf {
    RBUSD_BINARY_PATH.get_or_init(build_rbusd_binary).clone()
}

fn build_rbusd_binary() -> PathBuf {
    let cargo = std::env::var("CARGO").expect("CARGO environment variable should be set");
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("examples package should be inside workspace root");
    let output = Command::new(cargo)
        .args(["build", "-p", "rbusd", "--bin", RBUSD_BINARY_NAME])
        .current_dir(workspace_root)
        .output()
        .expect("rbusd build should start successfully");
    assert!(
        output.status.success(),
        "rbusd build failed: status={} stdout={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let current_executable =
        std::env::current_exe().expect("test executable path should be readable");
    let debug_directory = current_executable
        .parent()
        .and_then(|directory| directory.parent())
        .expect("test executable should be in target/debug/deps");
    debug_directory.join(RBUSD_BINARY_NAME)
}
