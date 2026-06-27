use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use rbus_ipc::control::client::request_control_frame;
use rbus_ipc::control::opcode::Opcode;
use rbus_ipc::unix_socket::default_socket_path;

static NEXT_DOMAIN_ID: AtomicU64 = AtomicU64::new(0);

pub struct DaemonProcess {
    child: Child,
    pub domain: String,
}

impl DaemonProcess {
    pub fn start(label: &str) -> Self {
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

pub fn unique_domain(label: &str) -> String {
    let domain_id = NEXT_DOMAIN_ID.fetch_add(1, Ordering::Relaxed);
    format!("rbus-{label}-{}-{domain_id}", std::process::id())
}

fn wait_until_daemon_accepts(domain: &str) {
    let socket_path = default_socket_path(domain);
    for _attempt in 0..100 {
        if request_control_frame(&socket_path, Opcode::Hello, 0, &[], 0).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
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
