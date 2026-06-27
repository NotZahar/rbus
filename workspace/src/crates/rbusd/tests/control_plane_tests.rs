use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use rbus_core::constants::CONTROL_PROTOCOL_VERSION;
use rbus_core::warning::WarningCode;
use rbus_ipc::control::client::{
    ControlClient, request_control_frame, request_control_frame_with_file_descriptors,
};
use rbus_ipc::control::io::CONTROL_FLAG_JSON;
use rbus_ipc::control::messages::{
    AttachStreamsRequest, AttachStreamsResponse, NOTIFY_STREAM_ADDED_NO_WAIT_NS,
    NotifyStreamAddedRequest, NotifyStreamAddedResponse, RegisterPublisherRequest,
    RegisterPublisherResponse, RegisterSubscriberRequest, RegisterSubscriberResponse,
    UnregisterRequest, UnregisterResponse, UnregisterTargetKind,
};
use rbus_ipc::control::opcode::Opcode;
use rbus_ipc::unix_socket::default_socket_path;

const TEST_TOPIC_HASH: u64 = 0x7262_7573_7465_7374;
const TEST_SLOT_SIZE: u64 = 64;
const TEST_DEPTH: u64 = 8;
const MAX_STREAM_RESPONSE_BYTES: usize = 1024;
const MAX_TEST_FILE_DESCRIPTORS: usize = 8;
const FIRST_PUBLISHER_ID: u64 = 1;
const FIRST_STREAM_ID: u64 = 1;
const FIRST_SUBSCRIBER_ID: u64 = 0;
const INITIAL_REGISTRY_GENERATION: u64 = 1;
const NO_SUBSCRIBERS: u64 = 0;
const ONE_FILE_DESCRIPTOR: usize = 1;
const TWO_FILE_DESCRIPTORS: usize = 2;
const NO_FILE_DESCRIPTORS: usize = 0;
const ONE_STREAM: usize = 1;
const REMOVED_ENTRY_COUNT: u64 = 1;
const UNKNOWN_PUBLISHER_ID: u64 = 1;
const INITIAL_NOTIFICATION_GENERATION: u64 = 0;
const WAIT_ATTEMPTS: usize = 100;
const WAIT_STEP: Duration = Duration::from_millis(10);

struct DaemonProcess {
    child: Child,
    domain: String,
}

impl DaemonProcess {
    fn start(domain: String) -> Self {
        let socket_path = default_socket_path(&domain);
        let _ = std::fs::remove_file(&socket_path);
        let child = Command::new(env!("CARGO_BIN_EXE_rbusd"))
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
fn daemon_answers_hello_frame() {
    let domain = unique_domain("hello");
    let daemon = DaemonProcess::start(domain);
    let response = request_control_frame(
        &default_socket_path(&daemon.domain),
        Opcode::Hello,
        0,
        &[],
        0,
    )
    .expect("rbusd should accept HELLO");

    assert_eq!(response.header.opcode, Opcode::Hello);
    assert_eq!(response.header.flags, CONTROL_PROTOCOL_VERSION);
    assert!(response.payload.is_empty());
}

#[test]
fn daemon_inspect_returns_json_snapshot() {
    let domain = unique_domain("inspect");
    let daemon = DaemonProcess::start(domain);
    let response = request_control_frame(
        &default_socket_path(&daemon.domain),
        Opcode::Inspect,
        CONTROL_FLAG_JSON,
        b"/control/cmd",
        1024,
    )
    .expect("operation should succeed");
    let payload = String::from_utf8(response.payload).expect("value should be utf8");

    assert_eq!(response.header.opcode, Opcode::Inspect);
    assert!(payload.contains("\"topic\":\"/control/cmd\""));
    assert!(payload.contains("\"streams\":[]"));
}

#[test]
fn register_publisher_happy_path_returns_stream_fd() {
    let daemon = DaemonProcess::start(unique_domain("register-publisher-ok"));
    let (response, file_descriptor_count, _session) = register_publisher(&daemon);

    assert_eq!(response.publisher_id, FIRST_PUBLISHER_ID);
    assert_eq!(response.stream_id, FIRST_STREAM_ID);
    assert_eq!(response.slot_size, TEST_SLOT_SIZE);
    assert_eq!(response.depth, TEST_DEPTH);
    assert_eq!(response.subscriber_count, NO_SUBSCRIBERS);
    assert_eq!(file_descriptor_count, TWO_FILE_DESCRIPTORS);
}

#[test]
fn register_publisher_warning_path_rejects_bad_payload() {
    let daemon = DaemonProcess::start(unique_domain("register-publisher-warning"));

    assert_runtime_warning(&daemon, Opcode::RegisterPublisher, &[]);
}

#[test]
fn register_subscriber_happy_path_returns_subscriber_id() {
    let daemon = DaemonProcess::start(unique_domain("register-subscriber-ok"));
    let (response, _session) = register_subscriber(&daemon);

    assert_eq!(response.subscriber_id, FIRST_SUBSCRIBER_ID);
    assert_eq!(response.subscribed_generation, INITIAL_REGISTRY_GENERATION);
}

#[test]
fn register_subscriber_warning_path_rejects_bad_payload() {
    let daemon = DaemonProcess::start(unique_domain("register-subscriber-warning"));

    assert_runtime_warning(&daemon, Opcode::RegisterSubscriber, &[]);
}

#[test]
fn attach_streams_happy_path_sends_registered_stream_fd() {
    let daemon = DaemonProcess::start(unique_domain("attach-streams-ok"));
    let (publisher, _, _publisher_session) = register_publisher(&daemon);
    let (response, file_descriptor_count) = attach_streams(&daemon);

    assert_eq!(response.streams.len(), ONE_STREAM);
    assert_eq!(response.streams[0].publisher_id, publisher.publisher_id);
    assert_eq!(response.streams[0].stream_id, publisher.stream_id);
    assert_eq!(file_descriptor_count, ONE_FILE_DESCRIPTOR);
}

#[test]
fn attach_streams_warning_path_rejects_bad_payload() {
    let daemon = DaemonProcess::start(unique_domain("attach-streams-warning"));

    assert_runtime_warning(&daemon, Opcode::AttachStreams, &[]);
}

#[test]
fn notify_stream_added_happy_path_sends_new_stream_fd() {
    let daemon = DaemonProcess::start(unique_domain("notify-stream-added-ok"));
    let (subscriber, _subscriber_session) = register_subscriber(&daemon);
    let (publisher, _, _publisher_session) = register_publisher(&daemon);
    let payload = NotifyStreamAddedRequest {
        topic_hash: TEST_TOPIC_HASH,
        subscriber_id: subscriber.subscriber_id,
        since_generation: subscriber.subscribed_generation,
        wait_timeout_ns: NOTIFY_STREAM_ADDED_NO_WAIT_NS,
    }
    .encode();
    let response = request_control_frame_with_file_descriptors(
        &default_socket_path(&daemon.domain),
        Opcode::NotifyStreamAdded,
        0,
        &payload,
        MAX_STREAM_RESPONSE_BYTES,
        MAX_TEST_FILE_DESCRIPTORS,
    )
    .expect("operation should succeed");
    let decoded = NotifyStreamAddedResponse::decode(&response.frame.payload)
        .expect("operation should succeed");

    assert_eq!(response.frame.header.opcode, Opcode::NotifyStreamAdded);
    assert!(decoded.latest_generation >= publisher.stream_generation);
    assert_eq!(decoded.streams.len(), ONE_STREAM);
    assert_eq!(decoded.streams[0].stream_id, publisher.stream_id);
    assert_eq!(response.file_descriptors.len(), ONE_FILE_DESCRIPTOR);
}

#[test]
fn notify_stream_added_warning_path_rejects_unknown_subscriber() {
    let daemon = DaemonProcess::start(unique_domain("notify-stream-added-warning"));
    let payload = NotifyStreamAddedRequest {
        topic_hash: TEST_TOPIC_HASH,
        subscriber_id: FIRST_SUBSCRIBER_ID,
        since_generation: INITIAL_NOTIFICATION_GENERATION,
        wait_timeout_ns: NOTIFY_STREAM_ADDED_NO_WAIT_NS,
    }
    .encode();

    assert_runtime_warning(&daemon, Opcode::NotifyStreamAdded, &payload);
}

#[test]
fn unregister_happy_path_removes_publisher_stream() {
    let daemon = DaemonProcess::start(unique_domain("unregister-ok"));
    let (publisher, _, _publisher_session) = register_publisher(&daemon);
    let payload = UnregisterRequest {
        target_kind: UnregisterTargetKind::Publisher,
        target_id: publisher.publisher_id,
    }
    .encode();
    let response = request_control_frame(
        &default_socket_path(&daemon.domain),
        Opcode::Unregister,
        0,
        &payload,
        UnregisterResponse::ENCODED_SIZE,
    )
    .expect("operation should succeed");
    let decoded = UnregisterResponse::decode(&response.payload).expect("operation should succeed");
    let (attached, file_descriptor_count) = attach_streams(&daemon);

    assert_eq!(response.header.opcode, Opcode::Unregister);
    assert_eq!(decoded.removed_count, REMOVED_ENTRY_COUNT);
    assert!(decoded.registry_generation > publisher.stream_generation);
    assert!(attached.streams.is_empty());
    assert_eq!(file_descriptor_count, NO_FILE_DESCRIPTORS);
}

#[test]
fn unregister_warning_path_rejects_unknown_publisher() {
    let daemon = DaemonProcess::start(unique_domain("unregister-warning"));
    let payload = UnregisterRequest {
        target_kind: UnregisterTargetKind::Publisher,
        target_id: UNKNOWN_PUBLISHER_ID,
    }
    .encode();

    assert_runtime_warning(&daemon, Opcode::Unregister, &payload);
}

#[test]
fn subscriber_id_is_reused_after_session_close() {
    let daemon = DaemonProcess::start(unique_domain("subscriber-lifecycle"));
    let (subscriber, session) = register_subscriber(&daemon);

    assert_eq!(subscriber.subscriber_id, FIRST_SUBSCRIBER_ID);
    drop(session);
    wait_until_first_subscriber_id_is_reused(&daemon);
}

#[test]
fn publisher_stream_is_removed_after_session_close() {
    let daemon = DaemonProcess::start(unique_domain("publisher-lifecycle"));
    let (_publisher, _file_descriptor_count, session) = register_publisher(&daemon);

    drop(session);
    wait_until_topic_has_no_streams(&daemon);
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

fn unique_domain(label: &str) -> String {
    format!("rbusd-{label}-{}", std::process::id())
}

fn register_publisher(daemon: &DaemonProcess) -> (RegisterPublisherResponse, usize, ControlClient) {
    let payload = RegisterPublisherRequest {
        topic_hash: TEST_TOPIC_HASH,
        max_message_size: TEST_SLOT_SIZE,
        depth: TEST_DEPTH,
    }
    .encode();
    let mut session = ControlClient::connect(&default_socket_path(&daemon.domain))
        .expect("operation should succeed");
    let response = session
        .request_frame_with_file_descriptors(
            Opcode::RegisterPublisher,
            0,
            &payload,
            RegisterPublisherResponse::ENCODED_SIZE,
            MAX_TEST_FILE_DESCRIPTORS,
        )
        .expect("operation should succeed");
    let decoded = RegisterPublisherResponse::decode(&response.frame.payload)
        .expect("operation should succeed");

    assert_eq!(response.frame.header.opcode, Opcode::RegisterPublisher);
    (decoded, response.file_descriptors.len(), session)
}

fn register_subscriber(daemon: &DaemonProcess) -> (RegisterSubscriberResponse, ControlClient) {
    let payload = RegisterSubscriberRequest {
        topic_hash: TEST_TOPIC_HASH,
    }
    .encode();
    let mut session = ControlClient::connect(&default_socket_path(&daemon.domain))
        .expect("operation should succeed");
    let response = session
        .request_frame_with_file_descriptors(
            Opcode::RegisterSubscriber,
            0,
            &payload,
            RegisterSubscriberResponse::ENCODED_SIZE,
            MAX_TEST_FILE_DESCRIPTORS,
        )
        .expect("subscriber_id should be released");

    assert_eq!(response.frame.header.opcode, Opcode::RegisterSubscriber);
    assert_eq!(response.file_descriptors.len(), ONE_FILE_DESCRIPTOR);
    (
        RegisterSubscriberResponse::decode(&response.frame.payload)
            .expect("operation should succeed"),
        session,
    )
}

fn attach_streams(daemon: &DaemonProcess) -> (AttachStreamsResponse, usize) {
    let payload = AttachStreamsRequest {
        topic_hash: TEST_TOPIC_HASH,
    }
    .encode();
    let response = request_control_frame_with_file_descriptors(
        &default_socket_path(&daemon.domain),
        Opcode::AttachStreams,
        0,
        &payload,
        MAX_STREAM_RESPONSE_BYTES,
        MAX_TEST_FILE_DESCRIPTORS,
    )
    .expect("operation should succeed");
    let decoded =
        AttachStreamsResponse::decode(&response.frame.payload).expect("operation should succeed");

    assert_eq!(response.frame.header.opcode, Opcode::AttachStreams);
    (decoded, response.file_descriptors.len())
}

fn assert_runtime_warning(daemon: &DaemonProcess, opcode: Opcode, payload: &[u8]) {
    let warning = request_control_frame(
        &default_socket_path(&daemon.domain),
        opcode,
        0,
        payload,
        MAX_STREAM_RESPONSE_BYTES,
    )
    .expect_err("operation should return warning");

    assert_eq!(warning.code(), WarningCode::Disconnected);
}

fn wait_until_first_subscriber_id_is_reused(daemon: &DaemonProcess) {
    for _attempt in 0..WAIT_ATTEMPTS {
        let (subscriber, session) = register_subscriber(daemon);
        if subscriber.subscriber_id == FIRST_SUBSCRIBER_ID {
            drop(session);
            return;
        }
        drop(session);
        thread::sleep(WAIT_STEP);
    }
    panic!("subscriber_id should be released");
}

fn wait_until_topic_has_no_streams(daemon: &DaemonProcess) {
    for _attempt in 0..WAIT_ATTEMPTS {
        let (attached, file_descriptor_count) = attach_streams(daemon);
        if attached.streams.is_empty() && file_descriptor_count == NO_FILE_DESCRIPTORS {
            return;
        }
        thread::sleep(WAIT_STEP);
    }
    panic!("publisher operation should succeed");
}
