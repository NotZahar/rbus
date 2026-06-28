use rbus::{Bus, TopicConfig, WarningCode};

mod common;

const FAST_PATH_TOPIC: &str = "/single/stream/fast-path";
const FAST_PATH_MAX_MESSAGE_SIZE: usize = 64;
const FAST_PATH_DEPTH: usize = 8;
const FIRST_PAYLOAD: &[u8] = b"first";
const SECOND_PAYLOAD: &[u8] = b"second";

#[test]
fn bus_connect_requires_running_daemon() {
    let domain = common::unique_domain("missing-daemon");
    let warning = match Bus::connect(&domain) {
        Ok(_) => panic!("operation should succeed"),
        Err(warning) => warning,
    };
    assert_eq!(warning.code(), WarningCode::DaemonUnavailable);
}

#[test]
fn bus_connect_accepts_daemon_domain() {
    let daemon = common::DaemonProcess::start("connect");
    Bus::connect(&daemon.domain).expect("domain should connect");
}

#[test]
fn bus_connect_rejects_empty_domain() {
    let warning = match Bus::connect("") {
        Ok(_) => panic!("domain should connect"),
        Err(warning) => warning,
    };
    assert_eq!(warning.code(), WarningCode::InvalidConfig);
}

#[test]
fn publisher_rejects_invalid_depth() {
    let daemon = common::DaemonProcess::start("invalid-depth");
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let warning = match bus.publisher("/topic", TopicConfig::new(1024, 63)) {
        Ok(_) => panic!("operation should return warning"),
        Err(warning) => warning,
    };
    assert_eq!(warning.code(), WarningCode::InvalidConfig);
}

#[test]
fn empty_try_recv_after_initial_attach_does_not_use_control_plane() {
    let daemon = common::DaemonProcess::start("empty-try-recv-data-plane");
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let subscriber = bus
        .subscriber("/empty/try_recv")
        .expect("subscriber operation should succeed");

    drop(daemon);
    let message = subscriber
        .try_recv()
        .expect("empty try_recv should stay in data plane");

    assert!(message.is_none());
}

#[test]
fn single_stream_fast_path_refreshes_new_stream_after_empty_read() {
    let daemon = common::DaemonProcess::start("single-stream-fast-path-refresh");
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let subscriber = bus
        .subscriber(FAST_PATH_TOPIC)
        .expect("subscriber operation should succeed");
    let mut first_publisher = bus
        .publisher(
            FAST_PATH_TOPIC,
            TopicConfig::new(FAST_PATH_MAX_MESSAGE_SIZE, FAST_PATH_DEPTH),
        )
        .expect("publisher operation should succeed");

    first_publisher
        .publish_bytes(FIRST_PAYLOAD)
        .expect("publish should succeed");
    let first_message = subscriber
        .try_recv()
        .expect("receive should succeed")
        .expect("first message should be available");
    assert_eq!(first_message.as_slice(), FIRST_PAYLOAD);
    drop(first_message);

    assert!(
        subscriber
            .try_recv()
            .expect("empty receive should succeed")
            .is_none()
    );

    let mut second_publisher = bus
        .publisher(
            FAST_PATH_TOPIC,
            TopicConfig::new(FAST_PATH_MAX_MESSAGE_SIZE, FAST_PATH_DEPTH),
        )
        .expect("publisher operation should succeed");
    second_publisher
        .publish_bytes(SECOND_PAYLOAD)
        .expect("publish should succeed");

    let second_message = subscriber
        .try_recv()
        .expect("receive should succeed")
        .expect("second publisher message should be available");
    assert_eq!(second_message.as_slice(), SECOND_PAYLOAD);
}
