use rbus::{Bus, TopicConfig};

mod common;

#[test]
fn loaned_publish_makes_payload_visible_to_subscriber() {
    let daemon = common::DaemonProcess::start("loaned-publish");
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let subscriber = bus
        .subscriber("/camera/front/image")
        .expect("subscriber operation should succeed");
    let publisher = bus
        .publisher("/camera/front/image", TopicConfig::new(1024, 8))
        .expect("publisher operation should succeed");

    let mut message = publisher.loan(4).expect("operation should succeed");
    message.as_mut_slice().copy_from_slice(&[1, 2, 3, 4]);
    let sequence = publisher.publish(message).expect("publish should succeed");

    let received_message = subscriber
        .try_recv()
        .expect("operation should return warning")
        .expect("message should be available");
    assert_eq!(received_message.sequence(), sequence);
    assert_eq!(received_message.as_slice(), &[1, 2, 3, 4]);
}
