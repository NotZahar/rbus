use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use rbus_core::warning::{Warning, WarningCode};
use rbus_ipc::control::client::request_control_frame;
use rbus_ipc::control::opcode::Opcode;
use rbus_ipc::unix_socket::default_socket_path;

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const DAEMON_START_ATTEMPTS: usize = 100;
const DAEMON_START_POLL_INTERVAL_MS: u64 = 10;
const NO_CONTROL_FLAGS: u16 = 0;
const NO_RESPONSE_PAYLOAD_BYTES: usize = 0;
const RBUSD_BINARY_NAME: &str = "rbusd";

/// Daemon runtime metadata
pub struct DaemonProcess {
    child: Child,
    domain: String,
}

impl DaemonProcess {
    /// Daemon runtime metadata
    pub fn start(domain: &str) -> Result<Self, Warning> {
        let child = Command::new(rbusd_binary_path()?)
            .args(["--domain", domain])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| warning_from_io(error, "DaemonProcess::spawn"))?;
        wait_until_daemon_accepts(domain)?;
        Ok(Self {
            child,
            domain: domain.to_owned(),
        })
    }
}

impl Drop for DaemonProcess {
    fn drop(&mut self) {
        let _kill_result = self.child.kill();
        let _wait_result = self.child.wait();
        let _remove_socket_result = std::fs::remove_file(default_socket_path(&self.domain));
    }
}

fn rbusd_binary_path() -> Result<PathBuf, Warning> {
    let current_executable =
        std::env::current_exe().map_err(|error| warning_from_io(error, "current_exe"))?;
    let executable_directory = current_executable
        .parent()
        .ok_or_else(|| warning("rbusd_binary_path::parent"))?;
    Ok(executable_directory.join(RBUSD_BINARY_NAME))
}

fn wait_until_daemon_accepts(domain: &str) -> Result<(), Warning> {
    let socket_path = default_socket_path(domain);
    for _attempt in 0..DAEMON_START_ATTEMPTS {
        if request_control_frame(
            &socket_path,
            Opcode::Hello,
            NO_CONTROL_FLAGS,
            &[],
            NO_RESPONSE_PAYLOAD_BYTES,
        )
        .is_ok()
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(DAEMON_START_POLL_INTERVAL_MS));
    }
    Err(warning("wait_until_daemon_accepts"))
}

fn warning(operation: &'static str) -> Warning {
    Warning::new(WarningCode::InvalidConfig, COMPONENT_RBUS_BENCH, operation)
}

fn warning_from_io(error: std::io::Error, operation: &'static str) -> Warning {
    match error.raw_os_error() {
        Some(errno) => Warning::new(WarningCode::SyscallFailed, COMPONENT_RBUS_BENCH, operation)
            .with_os_errno(errno),
        None => Warning::new(WarningCode::SyscallFailed, COMPONENT_RBUS_BENCH, operation),
    }
}
