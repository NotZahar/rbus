use rbus::{Bus, Sequence, TopicConfig, WarningCode};

mod common;

const GAP_TEST_TOPIC: &str = "/control/gap";
const GAP_TEST_MAX_MESSAGE_SIZE: usize = 1;
const GAP_TEST_STREAM_DEPTH: usize = 2;
const GAP_TEST_FIRST_PUBLISHED_PAYLOAD: u8 = 0;
const GAP_TEST_PUBLISHED_MESSAGE_COUNT: u8 = 4;
const GAP_TEST_FIRST_MISSING_SEQUENCE: u64 = 0;
const GAP_TEST_FIRST_AVAILABLE_SEQUENCE: u64 = 2;
const GAP_TEST_SECOND_AVAILABLE_SEQUENCE: u64 = 3;
const GAP_TEST_FIRST_AVAILABLE_PAYLOAD: &[u8] = &[2];
const GAP_TEST_SECOND_AVAILABLE_PAYLOAD: &[u8] = &[3];

#[test]
fn publish_bytes_copies_payload_once_into_stream_slot() {
    let daemon = common::DaemonProcess::start("publish-bytes");
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let subscriber = bus
        .subscriber("/control/cmd")
        .expect("subscriber operation should succeed");
    let mut publisher = bus
        .publisher("/control/cmd", TopicConfig::new(64, 8))
        .expect("publisher operation should succeed");

    publisher
        .publish_bytes(&[9, 8, 7])
        .expect("publish should succeed");
    let message = subscriber
        .try_recv()
        .expect("operation should return warning")
        .expect("message should be available");
    assert_eq!(message.as_slice(), &[9, 8, 7]);
}

#[test]
fn subscriber_cursor_moves_to_oldest_available_message_after_gap_warning() {
    let daemon = common::DaemonProcess::start("publish-bytes-gap");
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let subscriber = bus
        .subscriber(GAP_TEST_TOPIC)
        .expect("subscriber operation should succeed");
    let mut publisher = bus
        .publisher(
            GAP_TEST_TOPIC,
            TopicConfig::new(GAP_TEST_MAX_MESSAGE_SIZE, GAP_TEST_STREAM_DEPTH),
        )
        .expect("publisher operation should succeed");

    for payload in GAP_TEST_FIRST_PUBLISHED_PAYLOAD..GAP_TEST_PUBLISHED_MESSAGE_COUNT {
        publisher
            .publish_bytes(&[payload])
            .expect("publish should succeed");
    }

    let warning = match subscriber.try_recv() {
        Ok(_) => panic!("operation should return warning"),
        Err(warning) => warning,
    };
    assert_eq!(warning.code(), WarningCode::MessagesSkipped);
    assert_eq!(
        warning.sequence(),
        Some(Sequence::new(GAP_TEST_FIRST_MISSING_SEQUENCE))
    );

    let first_message = subscriber
        .try_recv()
        .expect("operation should return warning")
        .expect("message should be available");
    assert_eq!(
        first_message.sequence(),
        Sequence::new(GAP_TEST_FIRST_AVAILABLE_SEQUENCE)
    );
    assert_eq!(first_message.as_slice(), GAP_TEST_FIRST_AVAILABLE_PAYLOAD);
    drop(first_message);

    let second_message = subscriber
        .try_recv()
        .expect("operation should return warning")
        .expect("message should be available");
    assert_eq!(
        second_message.sequence(),
        Sequence::new(GAP_TEST_SECOND_AVAILABLE_SEQUENCE)
    );
    assert_eq!(second_message.as_slice(), GAP_TEST_SECOND_AVAILABLE_PAYLOAD);
}

#[test]
fn subscriber_created_after_publish_reads_only_new_messages() {
    let daemon = common::DaemonProcess::start("publish-after-subscriber");
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let mut publisher = bus
        .publisher("/control/cmd", TopicConfig::new(64, 8))
        .expect("publisher operation should succeed");
    publisher
        .publish_bytes(&[1])
        .expect("message should be available");

    let subscriber = bus
        .subscriber("/control/cmd")
        .expect("subscriber operation should succeed");
    assert!(
        subscriber
            .try_recv()
            .expect("operation should return warning")
            .is_none(),
        "subscriber operation should succeed"
    );

    publisher
        .publish_bytes(&[2])
        .expect("message should be available");
    let message = subscriber
        .try_recv()
        .expect("operation should return warning")
        .expect("message should be available");
    assert_eq!(message.as_slice(), &[2]);
}

#[test]
fn publish_bytes_rejects_oversized_message() {
    let daemon = common::DaemonProcess::start("publish-bytes-large");
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let mut publisher = bus
        .publisher("/control/cmd", TopicConfig::new(2, 8))
        .expect("publisher operation should succeed");
    let warning = match publisher.publish_bytes(&[1, 2, 3]) {
        Ok(_) => panic!("operation should return warning"),
        Err(warning) => warning,
    };
    assert_eq!(warning.code(), WarningCode::MessageTooLarge);
}
