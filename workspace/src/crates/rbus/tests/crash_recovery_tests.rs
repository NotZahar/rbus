use std::os::fd::RawFd;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use rbus::{Bus, Publisher, Subscriber, TopicConfig, WarningCode};
use rbus_ipc::unix_socket::default_socket_path;

mod common;

const SUBSCRIBER_CRASH_TOPIC: &str = "/crash/subscriber";
const PUBLISHER_CRASH_TOPIC: &str = "/crash/publisher";
const RESTART_TOPIC: &str = "/crash/restart";
const SLOT_SIZE: usize = 64;
const SINGLE_SLOT_DEPTH: usize = 1;
const TEST_DEPTH: usize = 8;
const CHILD_PROCESS_PID: libc::pid_t = 0;
const INVALID_FILE_DESCRIPTOR: RawFd = -1;
const PIPE_READ_INDEX: usize = 0;
const PIPE_WRITE_INDEX: usize = 1;
const PIPE_FILE_DESCRIPTOR_COUNT: usize = 2;
const PIPE_BYTE_COUNT: usize = 1;
const POLL_FILE_DESCRIPTOR_COUNT: libc::nfds_t = 1;
const POLL_READY_DESCRIPTOR_COUNT: i32 = 1;
const SUCCESS_RETURN_CODE: i32 = 0;
const INITIAL_WAIT_STATUS: i32 = 0;
const CLOSED_STREAM_COUNT: u64 = 1;
const PIPE_TIMEOUT_MS: i32 = 2_000;
const WAIT_ATTEMPTS: usize = 200;
const WAIT_STEP: Duration = Duration::from_millis(10);
const CHILD_RECEIVE_TIMEOUT: Duration = Duration::from_millis(1_000);
const CHILD_HOLD_SLEEP: Duration = Duration::from_secs(1);
const CHILD_READY_BYTE: u8 = b'R';
const CHILD_PINNED_BYTE: u8 = b'P';
const CHILD_PUBLISHED_BYTE: u8 = b'M';
const SUBSCRIBER_CRASH_FIRST_PAYLOAD: &[u8] = b"held";
const SUBSCRIBER_CRASH_RECOVERY_PAYLOAD: &[u8] = b"after-cleanup";
const PUBLISHER_CRASH_PAYLOAD: &[u8] = b"before-crash";
const RESTART_FIRST_PAYLOAD: &[u8] = b"before-restart";
const RESTART_SECOND_PAYLOAD: &[u8] = b"after-restart";

#[test]
fn rbusd_cleanup_clears_reader_bit_after_subscriber_crash() {
    let daemon = common::DaemonProcess::start("subscriber-crash");
    let bus = Bus::connect(&daemon.domain).expect("publisher operation should succeed");
    let publisher = bus
        .publisher(
            SUBSCRIBER_CRASH_TOPIC,
            TopicConfig::new(SLOT_SIZE, SINGLE_SLOT_DEPTH),
        )
        .expect("publisher operation should succeed");
    let mut pipe = Pipe::new();
    let child_pid = fork_process();

    if child_pid == CHILD_PROCESS_PID {
        pipe.close_read();
        child_subscriber_holds_message_ref(&daemon.domain, SUBSCRIBER_CRASH_TOPIC, pipe.write_fd);
    }

    pipe.close_write();
    assert_eq!(pipe.read_byte(), CHILD_READY_BYTE);
    publisher
        .publish_bytes(SUBSCRIBER_CRASH_FIRST_PAYLOAD)
        .expect("message should be available");
    assert_eq!(pipe.read_byte(), CHILD_PINNED_BYTE);
    kill_child_and_wait(child_pid);

    wait_until_publish_succeeds(&publisher, SUBSCRIBER_CRASH_RECOVERY_PAYLOAD);
}

#[test]
fn publisher_crash_marks_stream_closed_without_breaking_subscriber() {
    let daemon = common::DaemonProcess::start("publisher-crash");
    let bus = Bus::connect(&daemon.domain).expect("subscriber operation should succeed");
    let subscriber = bus
        .subscriber(PUBLISHER_CRASH_TOPIC)
        .expect("subscriber operation should succeed");
    let mut pipe = Pipe::new();
    let child_pid = fork_process();

    if child_pid == CHILD_PROCESS_PID {
        pipe.close_read();
        child_publisher_publishes_and_waits(&daemon.domain, PUBLISHER_CRASH_TOPIC, pipe.write_fd);
    }

    pipe.close_write();
    assert_eq!(pipe.read_byte(), CHILD_PUBLISHED_BYTE);
    let message = subscriber
        .recv_timeout(CHILD_RECEIVE_TIMEOUT)
        .expect("subscriber operation should succeed")
        .expect("publisher operation should succeed");
    assert_eq!(message.as_slice(), PUBLISHER_CRASH_PAYLOAD);
    drop(message);

    kill_child_and_wait(child_pid);
    wait_until_stream_is_closed(&subscriber);
    assert!(
        subscriber
            .try_recv()
            .expect("receive should succeed")
            .is_none()
    );
}

#[test]
fn rbusd_restart_keeps_existing_data_plane_working() {
    let mut daemon = RestartableDaemonProcess::start("rbusd-restart");
    let bus = Bus::connect(&daemon.domain).expect("operation should succeed");
    let subscriber = bus
        .subscriber(RESTART_TOPIC)
        .expect("subscriber operation should succeed");
    let publisher = bus
        .publisher(RESTART_TOPIC, TopicConfig::new(SLOT_SIZE, TEST_DEPTH))
        .expect("publisher operation should succeed");

    publisher
        .publish_bytes(RESTART_FIRST_PAYLOAD)
        .expect("message should be available");
    let first_message = subscriber
        .recv_timeout(CHILD_RECEIVE_TIMEOUT)
        .expect("subscriber operation should succeed")
        .expect("message should be available");
    assert_eq!(first_message.as_slice(), RESTART_FIRST_PAYLOAD);
    drop(first_message);

    daemon.restart();
    publisher
        .publish_bytes(RESTART_SECOND_PAYLOAD)
        .expect("operation should succeed");
    let second_message = subscriber
        .recv_timeout(CHILD_RECEIVE_TIMEOUT)
        .expect("subscriber operation should succeed")
        .expect("message should be available");
    assert_eq!(second_message.as_slice(), RESTART_SECOND_PAYLOAD);
}

fn child_subscriber_holds_message_ref(domain: &str, topic: &str, write_fd: RawFd) -> ! {
    let bus = Bus::connect(domain).unwrap_or_else(|_| child_exit());
    let subscriber = bus.subscriber(topic).unwrap_or_else(|_| child_exit());
    write_pipe_byte(write_fd, CHILD_READY_BYTE);
    let message = subscriber
        .recv_timeout(CHILD_RECEIVE_TIMEOUT)
        .unwrap_or_else(|_| child_exit())
        .unwrap_or_else(|| child_exit());
    if message.as_slice() != SUBSCRIBER_CRASH_FIRST_PAYLOAD {
        child_exit();
    }
    write_pipe_byte(write_fd, CHILD_PINNED_BYTE);
    loop {
        thread::sleep(CHILD_HOLD_SLEEP);
        std::hint::black_box(message.as_slice());
    }
}

struct RestartableDaemonProcess {
    child: Child,
    domain: String,
}

impl RestartableDaemonProcess {
    fn start(label: &str) -> Self {
        let domain = common::unique_domain(label);
        let _ = std::fs::remove_file(default_socket_path(&domain));
        let child = spawn_rbusd(&domain);
        wait_until_daemon_accepts(&domain);
        Self { child, domain }
    }

    fn restart(&mut self) {
        self.terminate_child();
        self.child = spawn_rbusd(&self.domain);
        wait_until_daemon_accepts(&self.domain);
    }

    fn terminate_child(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for RestartableDaemonProcess {
    fn drop(&mut self) {
        self.terminate_child();
        let _ = std::fs::remove_file(default_socket_path(&self.domain));
    }
}

fn spawn_rbusd(domain: &str) -> Child {
    Command::new(rbusd_binary_path())
        .args(["--domain", domain])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("rbusd should run successfully")
}

fn wait_until_daemon_accepts(domain: &str) {
    for _attempt in 0..WAIT_ATTEMPTS {
        if Bus::connect(domain).is_ok() {
            return;
        }
        thread::sleep(WAIT_STEP);
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

fn child_publisher_publishes_and_waits(domain: &str, topic: &str, write_fd: RawFd) -> ! {
    let bus = Bus::connect(domain).unwrap_or_else(|_| child_exit());
    let publisher = bus
        .publisher(topic, TopicConfig::new(SLOT_SIZE, TEST_DEPTH))
        .unwrap_or_else(|_| child_exit());
    publisher
        .publish_bytes(PUBLISHER_CRASH_PAYLOAD)
        .unwrap_or_else(|_| child_exit());
    write_pipe_byte(write_fd, CHILD_PUBLISHED_BYTE);
    loop {
        thread::sleep(CHILD_HOLD_SLEEP);
    }
}

fn wait_until_publish_succeeds(publisher: &Publisher, payload: &[u8]) {
    for _attempt in 0..WAIT_ATTEMPTS {
        match publisher.publish_bytes(payload) {
            Ok(_sequence) => return,
            Err(warning) if warning.code() == WarningCode::BackpressurePinnedSlot => {
                thread::sleep(WAIT_STEP);
            }
            Err(warning) => panic!("unexpected publish warning: {:?}", warning),
        }
    }
    panic!("publisher operation should succeed");
}

fn wait_until_stream_is_closed(subscriber: &Subscriber) {
    for _attempt in 0..WAIT_ATTEMPTS {
        let stats = subscriber.stats();
        if stats.connected_streams == CLOSED_STREAM_COUNT
            && stats.disconnected_streams == CLOSED_STREAM_COUNT
            && !stats.publisher_alive
        {
            return;
        }
        thread::sleep(WAIT_STEP);
    }
    panic!("subscriber operation should succeed");
}

fn fork_process() -> libc::pid_t {
    // SAFETY: Required invariants are upheld
    // Internal note
    let child_pid = unsafe { libc::fork() };
    assert!(child_pid >= CHILD_PROCESS_PID, "fork should succeed");
    child_pid
}

fn kill_child_and_wait(child_pid: libc::pid_t) {
    // SAFETY: Required invariants are upheld
    let kill_result = unsafe { libc::kill(child_pid, libc::SIGKILL) };
    assert_eq!(kill_result, SUCCESS_RETURN_CODE, "kill should succeed");
    let mut status = INITIAL_WAIT_STATUS;
    // SAFETY: Required invariants are upheld
    let waited_pid = unsafe { libc::waitpid(child_pid, &mut status, 0) };
    assert_eq!(
        waited_pid, child_pid,
        "waitpid should observe child process"
    );
    assert!(
        libc::WIFSIGNALED(status),
        "child process should exit as expected"
    );
    assert_eq!(
        libc::WTERMSIG(status),
        libc::SIGKILL,
        "child process should exit as expected"
    );
}

fn child_exit() -> ! {
    // SAFETY: Required invariants are upheld
    unsafe {
        libc::_exit(libc::EXIT_FAILURE);
    }
}

fn write_pipe_byte(file_descriptor: RawFd, byte: u8) {
    let buffer = [byte];
    // SAFETY: Required invariants are upheld
    let written = unsafe {
        libc::write(
            file_descriptor,
            buffer.as_ptr().cast::<libc::c_void>(),
            PIPE_BYTE_COUNT,
        )
    };
    if written != PIPE_BYTE_COUNT as isize {
        child_exit();
    }
}

struct Pipe {
    read_fd: RawFd,
    write_fd: RawFd,
}

impl Pipe {
    fn new() -> Self {
        let mut file_descriptors = [INVALID_FILE_DESCRIPTOR; PIPE_FILE_DESCRIPTOR_COUNT];
        // SAFETY: Required invariants are upheld
        let result = unsafe { libc::pipe(file_descriptors.as_mut_ptr()) };
        assert_eq!(result, SUCCESS_RETURN_CODE, "pipe operation should succeed");
        Self {
            read_fd: file_descriptors[PIPE_READ_INDEX],
            write_fd: file_descriptors[PIPE_WRITE_INDEX],
        }
    }

    fn read_byte(&self) -> u8 {
        let mut poll_descriptor = libc::pollfd {
            fd: self.read_fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: Required invariants are upheld
        let poll_result = unsafe {
            libc::poll(
                &mut poll_descriptor,
                POLL_FILE_DESCRIPTOR_COUNT,
                PIPE_TIMEOUT_MS,
            )
        };
        assert_eq!(
            poll_result, POLL_READY_DESCRIPTOR_COUNT,
            "pipe operation should succeed"
        );
        let mut byte = 0_u8;
        // SAFETY: Required invariants are upheld
        let read_count = unsafe {
            libc::read(
                self.read_fd,
                core::ptr::addr_of_mut!(byte).cast::<libc::c_void>(),
                PIPE_BYTE_COUNT,
            )
        };
        assert_eq!(
            read_count, PIPE_BYTE_COUNT as isize,
            "pipe operation should succeed"
        );
        byte
    }

    fn close_read(&mut self) {
        close_file_descriptor(&mut self.read_fd);
    }

    fn close_write(&mut self) {
        close_file_descriptor(&mut self.write_fd);
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        close_file_descriptor(&mut self.read_fd);
        close_file_descriptor(&mut self.write_fd);
    }
}

fn close_file_descriptor(file_descriptor: &mut RawFd) {
    if *file_descriptor != INVALID_FILE_DESCRIPTOR {
        // SAFETY: Required invariants are upheld
        unsafe {
            libc::close(*file_descriptor);
        }
        *file_descriptor = INVALID_FILE_DESCRIPTOR;
    }
}
