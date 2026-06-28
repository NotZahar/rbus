use rbus::{Bus, TopicConfig, WarningCode};

mod common;

#[test]
fn message_ref_drop_releases_pinned_slot() {
    let daemon = common::DaemonProcess::start("message-ref-drop");
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let subscriber = bus
        .subscriber("/topic")
        .expect("subscriber operation should succeed");
    let mut publisher = bus
        .publisher("/topic", TopicConfig::new(16, 1))
        .expect("publisher operation should succeed");

    publisher
        .publish_bytes(&[1])
        .expect("publish should succeed");
    let message = subscriber
        .try_recv()
        .expect("operation should return warning")
        .expect("message should be available");

    let warning = match publisher.publish_bytes(&[2]) {
        Ok(_) => panic!("slot should be released"),
        Err(warning) => warning,
    };
    assert_eq!(warning.code(), WarningCode::BackpressurePinnedSlot);

    drop(message);
    publisher
        .publish_bytes(&[3])
        .expect("slot should be released");
}
