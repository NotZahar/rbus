use std::time::Duration;

use rbus::{Bus, TopicConfig};

mod common;

const LATE_PUBLISHER_DELAY: Duration = Duration::from_millis(20);
const BLOCKING_RECV_DEADLINE: Duration = Duration::from_millis(500);

#[test]
fn recv_timeout_returns_none_when_no_message_arrives() {
    let daemon = common::DaemonProcess::start("recv-timeout");
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let subscriber = bus
        .subscriber("/topic")
        .expect("subscriber operation should succeed");
    let message = subscriber
        .recv_timeout(Duration::from_millis(2))
        .expect("timeout should not expire");
    assert!(message.is_none());
}

#[test]
fn recv_timeout_returns_message_before_deadline() {
    let daemon = common::DaemonProcess::start("recv-timeout-message");
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let subscriber = bus
        .subscriber("/topic")
        .expect("subscriber operation should succeed");
    let mut publisher = bus
        .publisher("/topic", TopicConfig::new(16, 8))
        .expect("publisher operation should succeed");
    publisher
        .publish_bytes(&[5])
        .expect("publish should succeed");

    let message = subscriber
        .recv_timeout(Duration::from_millis(20))
        .expect("operation should return warning")
        .expect("message should be available");
    assert_eq!(message.as_slice(), &[5]);
}

#[test]
fn recv_timeout_wakes_when_publisher_appears_after_subscriber_waits() {
    let daemon = common::DaemonProcess::start("recv-timeout-late-publisher");
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let subscriber = bus
        .subscriber("/topic")
        .expect("subscriber operation should succeed");
    let publisher_domain = daemon.domain.clone();
    let publisher_thread = std::thread::spawn(move || {
        std::thread::sleep(LATE_PUBLISHER_DELAY);
        let bus = Bus::connect(&publisher_domain).expect("domain should connect");
        let mut publisher = bus
            .publisher("/topic", TopicConfig::new(16, 8))
            .expect("publisher operation should succeed");
        publisher
            .publish_bytes(&[7])
            .expect("publish should succeed");
    });

    let message = subscriber
        .recv_timeout(BLOCKING_RECV_DEADLINE)
        .expect("operation should return warning")
        .expect("message should be available");
    publisher_thread
        .join()
        .expect("publisher operation should succeed");

    assert_eq!(message.as_slice(), &[7]);
}
