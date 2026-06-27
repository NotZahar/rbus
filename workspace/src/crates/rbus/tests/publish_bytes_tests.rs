use rbus::{Bus, TopicConfig, WarningCode};

mod common;

#[test]
fn publish_bytes_copies_payload_once_into_stream_slot() {
    let daemon = common::DaemonProcess::start("publish-bytes");
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let subscriber = bus
        .subscriber("/control/cmd")
        .expect("subscriber operation should succeed");
    let publisher = bus
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
fn subscriber_created_after_publish_reads_only_new_messages() {
    let daemon = common::DaemonProcess::start("publish-after-subscriber");
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let publisher = bus
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
    let publisher = bus
        .publisher("/control/cmd", TopicConfig::new(2, 8))
        .expect("publisher operation should succeed");
    let warning = match publisher.publish_bytes(&[1, 2, 3]) {
        Ok(_) => panic!("operation should return warning"),
        Err(warning) => warning,
    };
    assert_eq!(warning.code(), WarningCode::MessageTooLarge);
}
