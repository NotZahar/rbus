use rbus::{Bus, Sequence, TopicConfig, WarningCode};
use rbus_ipc::unix_socket::default_socket_path;

mod common;

const TEST_SLOT_SIZE: usize = 8;
const TEST_DEPTH: usize = 4;
const SINGLE_SLOT_DEPTH: usize = 1;
const OVERSIZED_PAYLOAD: &[u8] = b"too-large";
const PINNED_PAYLOAD: &[u8] = b"a";
const RECOVERY_PAYLOAD: &[u8] = b"b";
const GAP_TOPIC: &str = "/warning/gap";
const GAP_SLOT_SIZE: usize = 1;
const GAP_DEPTH: usize = 2;
const GAP_PUBLISHED_MESSAGES: u8 = 4;
const GAP_SKIPPED_MESSAGES: u64 = 2;
const GAP_FIRST_AVAILABLE_SEQUENCE: u64 = 2;

#[test]
fn invalid_config_warning_path_preserves_empty_runtime_state() {
    let warning = match Bus::connect("") {
        Ok(_) => panic!("operation should return warning"),
        Err(warning) => warning,
    };

    assert_eq!(warning.code(), WarningCode::InvalidConfig);
    assert!(!default_socket_path("").exists());
}

#[test]
fn message_too_large_warning_path_increments_total_only() {
    let daemon = common::DaemonProcess::start("warning-large");
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let publisher = bus
        .publisher(
            "/warning/large",
            TopicConfig::new(TEST_SLOT_SIZE, TEST_DEPTH),
        )
        .expect("publisher operation should succeed");

    let warning = match publisher.publish_bytes(OVERSIZED_PAYLOAD) {
        Ok(_) => panic!("operation should return warning"),
        Err(warning) => warning,
    };
    let stats = publisher.stats();

    assert_eq!(warning.code(), WarningCode::MessageTooLarge);
    assert_eq!(stats.warnings_total, 1);
    assert_eq!(stats.published_messages, 0);
    assert_eq!(stats.backpressure_pinned_slot, 0);
    assert_eq!(stats.current_sequence, 0);
    assert!(stats.publisher_alive);
}

#[test]
fn backpressure_warning_path_increments_backpressure_counter() {
    let daemon = common::DaemonProcess::start("warning-backpressure");
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let subscriber = bus
        .subscriber("/warning/backpressure")
        .expect("subscriber operation should succeed");
    let publisher = bus
        .publisher(
            "/warning/backpressure",
            TopicConfig::new(TEST_SLOT_SIZE, SINGLE_SLOT_DEPTH),
        )
        .expect("publisher operation should succeed");

    publisher
        .publish_bytes(PINNED_PAYLOAD)
        .expect("publish should succeed");
    let message = subscriber
        .try_recv()
        .expect("receive should succeed")
        .expect("message should be available");
    let warning = match publisher.publish_bytes(RECOVERY_PAYLOAD) {
        Ok(_) => panic!("operation should return warning"),
        Err(warning) => warning,
    };
    let blocked_stats = publisher.stats();

    assert_eq!(warning.code(), WarningCode::BackpressurePinnedSlot);
    assert_eq!(blocked_stats.warnings_total, 1);
    assert_eq!(blocked_stats.backpressure_pinned_slot, 1);
    assert_eq!(blocked_stats.published_messages, 1);
    assert_eq!(blocked_stats.current_sequence, 1);

    drop(message);
    publisher
        .publish_bytes(RECOVERY_PAYLOAD)
        .expect("slot should be released");
    let recovered_stats = publisher.stats();
    assert_eq!(recovered_stats.warnings_total, 1);
    assert_eq!(recovered_stats.backpressure_pinned_slot, 1);
    assert_eq!(recovered_stats.published_messages, 2);
    assert_eq!(recovered_stats.current_sequence, 2);
}

#[test]
fn messages_skipped_warning_path_updates_cursor_and_counters() {
    let daemon = common::DaemonProcess::start("warning-gap");
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let subscriber = bus
        .subscriber(GAP_TOPIC)
        .expect("subscriber operation should succeed");
    let publisher = bus
        .publisher(GAP_TOPIC, TopicConfig::new(GAP_SLOT_SIZE, GAP_DEPTH))
        .expect("publisher operation should succeed");

    for payload in 0..GAP_PUBLISHED_MESSAGES {
        publisher
            .publish_bytes(&[payload])
            .expect("publish should succeed");
    }

    let warning = match subscriber.try_recv() {
        Ok(_) => panic!("operation should return warning"),
        Err(warning) => warning,
    };
    let warning_stats = subscriber.stats();
    let publisher_stats = publisher.stats();

    assert_eq!(warning.code(), WarningCode::MessagesSkipped);
    assert_eq!(warning.sequence(), Some(Sequence::new(0)));
    assert_eq!(warning_stats.warnings_total, 1);
    assert_eq!(warning_stats.skipped_messages, GAP_SKIPPED_MESSAGES);
    assert_eq!(
        publisher_stats.current_sequence,
        u64::from(GAP_PUBLISHED_MESSAGES)
    );

    let message = subscriber
        .try_recv()
        .expect("receive should succeed")
        .expect("message should be available");
    assert_eq!(
        message.sequence(),
        Sequence::new(GAP_FIRST_AVAILABLE_SEQUENCE)
    );
}

#[test]
fn internal_invariant_warning_path_rolls_back_foreign_loan() {
    let daemon = common::DaemonProcess::start("warning-invariant");
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let first_publisher = bus
        .publisher(
            "/warning/invariant",
            TopicConfig::new(TEST_SLOT_SIZE, TEST_DEPTH),
        )
        .expect("publisher operation should succeed");
    let second_publisher = bus
        .publisher(
            "/warning/invariant",
            TopicConfig::new(TEST_SLOT_SIZE, TEST_DEPTH),
        )
        .expect("publisher operation should succeed");
    let loan = first_publisher
        .loan(PINNED_PAYLOAD.len())
        .expect("loan should succeed");

    let warning = match second_publisher.publish(loan) {
        Ok(_) => panic!("operation should return warning"),
        Err(warning) => warning,
    };

    assert_eq!(warning.code(), WarningCode::InternalInvariantWarning);
    assert_eq!(first_publisher.stats().current_sequence, 0);
    first_publisher
        .publish_bytes(PINNED_PAYLOAD)
        .expect("rollback should release the original stream");
    assert_eq!(first_publisher.stats().published_messages, 1);
}
