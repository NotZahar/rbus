use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::Duration;

use rbus_ipc::control::client::request_control_frame;
use rbus_ipc::control::opcode::Opcode;
use rbus_ipc::unix_socket::default_socket_path;

const WARNING_EXIT_CODE: i32 = 2;
const DAEMON_WAIT_ATTEMPTS: usize = 100;
const DAEMON_WAIT_STEP: Duration = Duration::from_millis(10);
const TEST_PAYLOAD_SIZE: usize = 16;
const TEST_DEPTH: usize = 8;
const FIRST_WORKER_INDEX: u8 = 0;
const ONE_PUBLISHER: u8 = 1;
const ONE_SUBSCRIBER: u8 = 1;
const ONE_MESSAGE: u32 = 1;

struct DaemonProcess {
    child: Child,
    domain: String,
}

impl DaemonProcess {
    fn start(label: &str) -> Self {
        let domain = format!("rbus-bench-{label}-{}", std::process::id());
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
fn bench_worker_recv_timeout_warning_path_is_fatal() {
    let daemon = DaemonProcess::start("timeout");
    let output = run_timeout_worker(&daemon.domain);

    assert_eq!(output.status.code(), Some(WARNING_EXIT_CODE));
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("Timeout"),
        "stderr should contain Timeout: {stderr}"
    );
    assert!(
        stderr.contains("stacktrace:"),
        "stderr should contain stacktrace: {stderr}"
    );
}

fn run_timeout_worker(domain: &str) -> Output {
    let arguments = vec![
        String::from("--suite-worker"),
        String::from("subscriber"),
        String::from("--domain"),
        domain.to_string(),
        String::from("--topic"),
        String::from("/bench/timeout"),
        String::from("--payload-size"),
        TEST_PAYLOAD_SIZE.to_string(),
        String::from("--depth"),
        TEST_DEPTH.to_string(),
        String::from("--worker-index"),
        FIRST_WORKER_INDEX.to_string(),
        String::from("--publisher-count"),
        ONE_PUBLISHER.to_string(),
        String::from("--subscriber-count"),
        ONE_SUBSCRIBER.to_string(),
        String::from("--messages"),
        ONE_MESSAGE.to_string(),
        String::from("--wait-mode"),
        String::from("recv-timeout"),
        String::from("--transport"),
        String::from("rbus-publish-bytes"),
    ];
    Command::new(env!("CARGO_BIN_EXE_rbus-bench"))
        .args(arguments)
        .output()
        .expect("rbus-bench should run successfully")
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
    let current_executable =
        std::env::current_exe().expect("test executable path should be readable");
    let debug_directory = current_executable
        .parent()
        .and_then(|directory| directory.parent())
        .expect("test executable should be in target/debug/deps");
    debug_directory.join("rbusd")
}
