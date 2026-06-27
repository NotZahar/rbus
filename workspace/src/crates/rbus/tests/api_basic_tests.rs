use rbus::{Bus, TopicConfig, WarningCode};

mod common;

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
