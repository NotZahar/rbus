use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use rbus::{Bus, Publisher, Subscriber, TopicConfig};
use rbus_core::warning::{Warning, WarningCode};
use rbus_ipc::control::client::request_control_frame;
use rbus_ipc::control::opcode::Opcode;
use rbus_ipc::errno::warning_from_errno;
use rbus_ipc::unix_socket::default_socket_path;

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const DAEMON_START_ATTEMPTS: usize = 100;
const DAEMON_START_POLL_INTERVAL_MS: u64 = 10;
const HOT_PATH_DEPTH: usize = 256;
const HOT_PATH_MEASURED_ITERATIONS: usize = 128;
const HOT_PATH_PAYLOAD_LENGTH: usize = 64;
const HOT_PATH_PAYLOAD_SENTINEL_BYTE: u8 = 0xA5;
const HOT_PATH_PAYLOAD_SENTINEL_INDEX: usize = 0;
const HOT_PATH_TOPIC: &str = "/bench/hot_path_syscalls";
const HOT_PATH_WARMUP_ITERATIONS: usize = 16;
const NO_CONTROL_FLAGS: u16 = 0;
const NO_RESPONSE_PAYLOAD_BYTES: usize = 0;
const PUBLISH_BEGIN_MARKER: &[u8] = b"RBUS_HOT_PATH_BEGIN:publish\n";
const PUBLISH_END_MARKER: &[u8] = b"RBUS_HOT_PATH_END:publish\n";
const RBUSD_BINARY_NAME: &str = "rbusd";
const RBUSD_DOMAIN_PREFIX: &str = "rbus-hot-path-syscall";
const TRY_RECV_BEGIN_MARKER: &[u8] = b"RBUS_HOT_PATH_BEGIN:try-recv\n";
const TRY_RECV_END_MARKER: &[u8] = b"RBUS_HOT_PATH_END:try-recv\n";

const _: () = assert!(HOT_PATH_DEPTH.is_power_of_two());
const _: () = assert!(HOT_PATH_DEPTH >= HOT_PATH_WARMUP_ITERATIONS + HOT_PATH_MEASURED_ITERATIONS);
const _: () = assert!(HOT_PATH_PAYLOAD_SENTINEL_INDEX < HOT_PATH_PAYLOAD_LENGTH);

/// Internal documentation
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HotPathOperation {
    /// `Publisher::loan` + `Publisher::publish`
    Publish,
    /// Subscriber runtime metadata
    TryRecv,
}

impl HotPathOperation {
    /// CLI metadata
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "publish" => Some(Self::Publish),
            "try-recv" => Some(Self::TryRecv),
            _ => None,
        }
    }

    /// CLI metadata
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Publish => "publish",
            Self::TryRecv => "try-recv",
        }
    }

    const fn begin_marker(self) -> &'static [u8] {
        match self {
            Self::Publish => PUBLISH_BEGIN_MARKER,
            Self::TryRecv => TRY_RECV_BEGIN_MARKER,
        }
    }

    const fn end_marker(self) -> &'static [u8] {
        match self {
            Self::Publish => PUBLISH_END_MARKER,
            Self::TryRecv => TRY_RECV_END_MARKER,
        }
    }
}

/// Internal documentation
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HotPathSyscallReport {
    operation: HotPathOperation,
    measured_iterations: usize,
    warmup_iterations: usize,
    payload_length: usize,
    depth: usize,
}

impl HotPathSyscallReport {
    const fn new(operation: HotPathOperation) -> Self {
        Self {
            operation,
            measured_iterations: HOT_PATH_MEASURED_ITERATIONS,
            warmup_iterations: HOT_PATH_WARMUP_ITERATIONS,
            payload_length: HOT_PATH_PAYLOAD_LENGTH,
            depth: HOT_PATH_DEPTH,
        }
    }

    /// Returns a value
    pub fn to_json(self) -> String {
        format!(
            concat!(
                "{{\"status\":\"ok\",\"probe\":\"hot-path-syscalls\",",
                "\"operation\":\"{}\",\"measured_iterations\":{},",
                "\"warmup_iterations\":{},\"payload_length\":{},\"depth\":{}}}"
            ),
            self.operation.as_str(),
            self.measured_iterations,
            self.warmup_iterations,
            self.payload_length,
            self.depth
        )
    }
}

/// Runs the operation
pub fn run_probe(operation: HotPathOperation) -> Result<HotPathSyscallReport, Warning> {
    let daemon = DaemonProcess::start(operation)?;
    let bus = Bus::connect(&daemon.domain)?;
    match operation {
        HotPathOperation::Publish => run_publish_probe(&bus)?,
        HotPathOperation::TryRecv => run_try_recv_probe(&bus)?,
    }
    Ok(HotPathSyscallReport::new(operation))
}

fn run_publish_probe(bus: &Bus) -> Result<(), Warning> {
    let subscriber = bus.subscriber(HOT_PATH_TOPIC)?;
    let publisher = bus.publisher(HOT_PATH_TOPIC, topic_config())?;

    publish_message(&publisher)?;
    drop(receive_required(&subscriber)?);
    for _iteration in 0..HOT_PATH_WARMUP_ITERATIONS {
        publish_message(&publisher)?;
    }

    write_stdout_marker(HotPathOperation::Publish.begin_marker())?;
    for _iteration in 0..HOT_PATH_MEASURED_ITERATIONS {
        publish_message(&publisher)?;
    }
    write_stdout_marker(HotPathOperation::Publish.end_marker())
}

fn run_try_recv_probe(bus: &Bus) -> Result<(), Warning> {
    let subscriber = bus.subscriber(HOT_PATH_TOPIC)?;
    let publisher = bus.publisher(HOT_PATH_TOPIC, topic_config())?;

    for _iteration in 0..HOT_PATH_WARMUP_ITERATIONS + HOT_PATH_MEASURED_ITERATIONS {
        publish_message(&publisher)?;
    }
    for _iteration in 0..HOT_PATH_WARMUP_ITERATIONS {
        let _message = receive_required(&subscriber)?;
    }

    write_stdout_marker(HotPathOperation::TryRecv.begin_marker())?;
    for _iteration in 0..HOT_PATH_MEASURED_ITERATIONS {
        let _message = receive_required(&subscriber)?;
    }
    write_stdout_marker(HotPathOperation::TryRecv.end_marker())
}

fn topic_config() -> TopicConfig {
    TopicConfig::new(HOT_PATH_PAYLOAD_LENGTH, HOT_PATH_DEPTH)
}

fn publish_message(publisher: &Publisher) -> Result<(), Warning> {
    let mut message = publisher.loan(HOT_PATH_PAYLOAD_LENGTH)?;
    message.as_mut_slice()[HOT_PATH_PAYLOAD_SENTINEL_INDEX] = HOT_PATH_PAYLOAD_SENTINEL_BYTE;
    let _sequence = publisher.publish(message)?;
    Ok(())
}

fn receive_required<'subscriber>(
    subscriber: &'subscriber Subscriber,
) -> Result<rbus::MessageRef<'subscriber>, Warning> {
    subscriber.try_recv()?.ok_or_else(|| {
        Warning::new(
            WarningCode::InternalInvariantWarning,
            COMPONENT_RBUS_BENCH,
            "receive_required",
        )
    })
}

fn write_stdout_marker(marker: &'static [u8]) -> Result<(), Warning> {
    // SAFETY: Required invariants are upheld
    let written = unsafe { libc::write(libc::STDOUT_FILENO, marker.as_ptr().cast(), marker.len()) };
    if written == marker.len() as isize {
        Ok(())
    } else if written < 0 {
        Err(warning_from_errno(
            WarningCode::SyscallFailed,
            COMPONENT_RBUS_BENCH,
            "write_stdout_marker",
        ))
    } else {
        Err(Warning::new(
            WarningCode::SyscallFailed,
            COMPONENT_RBUS_BENCH,
            "write_stdout_marker::partial",
        ))
    }
}

struct DaemonProcess {
    child: Child,
    domain: String,
}

impl DaemonProcess {
    fn start(operation: HotPathOperation) -> Result<Self, Warning> {
        let domain = format!(
            "{RBUSD_DOMAIN_PREFIX}-{}-{}",
            operation.as_str(),
            std::process::id()
        );
        let child = Command::new(rbusd_binary_path()?)
            .args(["--domain", &domain])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| warning_from_io(error, WarningCode::DaemonUnavailable, "spawn"))?;
        wait_until_daemon_accepts(&domain)?;
        Ok(Self { child, domain })
    }
}

impl Drop for DaemonProcess {
    fn drop(&mut self) {
        let _kill_result = self.child.kill();
        let _wait_result = self.child.wait();
        let _remove_socket_result = std::fs::remove_file(default_socket_path(&self.domain));
    }
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
        thread::sleep(Duration::from_millis(DAEMON_START_POLL_INTERVAL_MS));
    }
    Err(Warning::new(
        WarningCode::DaemonUnavailable,
        COMPONENT_RBUS_BENCH,
        "wait_until_daemon_accepts",
    ))
}

fn rbusd_binary_path() -> Result<PathBuf, Warning> {
    let current_executable = std::env::current_exe()
        .map_err(|error| warning_from_io(error, WarningCode::InvalidConfig, "current_exe"))?;
    let executable_directory = current_executable.parent().ok_or_else(|| {
        Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_RBUS_BENCH,
            "rbusd_binary_path::parent",
        )
    })?;
    Ok(executable_directory.join(RBUSD_BINARY_NAME))
}

fn warning_from_io(
    error: std::io::Error,
    warning_code: WarningCode,
    operation: &'static str,
) -> Warning {
    match error.raw_os_error() {
        Some(errno) => {
            Warning::new(warning_code, COMPONENT_RBUS_BENCH, operation).with_os_errno(errno)
        }
        None => Warning::new(warning_code, COMPONENT_RBUS_BENCH, operation),
    }
}
