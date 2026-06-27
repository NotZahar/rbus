use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Duration;

use rbus::{Bus, TopicConfig};

mod common;

const CHILD_ROLE_ENV: &str = "RBUS_CROSS_PROCESS_ROLE";
const CHILD_ROLE_PUBLISHER: &str = "publisher";
const CHILD_ROLE_SUBSCRIBER: &str = "subscriber";
const DOMAIN_ENV: &str = "RBUS_CROSS_PROCESS_DOMAIN";
const TOPIC_ENV: &str = "RBUS_CROSS_PROCESS_TOPIC";
const PAYLOAD_ENV: &str = "RBUS_CROSS_PROCESS_PAYLOAD";
const EXPECTED_MESSAGES_ENV: &str = "RBUS_CROSS_PROCESS_EXPECTED_MESSAGES";
const PUBLISH_COMMAND: &str = "publish";
const EXIT_COMMAND: &str = "exit";
const PUBLISHER_READY_LINE: &str = "publisher-ready";
const SUBSCRIBER_READY_LINE: &str = "subscriber-ready";
const PUBLISHED_LINE: &str = "published";
const MESSAGE_PREFIX: &str = "message:";
const SUBSCRIBER_UNUSED_PAYLOAD: &str = "";
const TEST_MAX_MESSAGE_SIZE: usize = 64;
const TEST_DEPTH: usize = 8;
const EOF_BYTES_READ: usize = 0;
const SINGLE_MESSAGE_COUNT: usize = 1;
const MULTI_SUBSCRIBER_COUNT: usize = 2;
const MULTI_PUBLISHER_MESSAGE_COUNT: usize = 2;
const CHILD_RECEIVE_TIMEOUT: Duration = Duration::from_secs(2);
const SUBSCRIBER_BEFORE_TOPIC: &str = "/cross/process/subscriber-before";
const SUBSCRIBER_AFTER_TOPIC: &str = "/cross/process/subscriber-after";
const MULTI_SUBSCRIBER_TOPIC: &str = "/cross/process/multi-subscriber";
const MULTI_PUBLISHER_TOPIC: &str = "/cross/process/multi-publisher";
const SUBSCRIBER_BEFORE_PAYLOAD: &str = "subscriber-before-publisher";
const SUBSCRIBER_AFTER_PAYLOAD: &str = "subscriber-after-publisher";
const MULTI_SUBSCRIBER_PAYLOAD: &str = "fanout";
const FIRST_PUBLISHER_PAYLOAD: &str = "publisher-one";
const SECOND_PUBLISHER_PAYLOAD: &str = "publisher-two";

#[test]
fn subscriber_before_publisher_receives_after_stream_notification() {
    let daemon = common::DaemonProcess::start("cross-process-subscriber-before");
    let mut subscriber = SubscriberProcess::spawn(
        &daemon.domain,
        SUBSCRIBER_BEFORE_TOPIC,
        SINGLE_MESSAGE_COUNT,
    );
    let mut publisher = PublisherProcess::spawn(
        &daemon.domain,
        SUBSCRIBER_BEFORE_TOPIC,
        SUBSCRIBER_BEFORE_PAYLOAD,
    );

    publisher.publish();
    assert_ordered_messages(subscriber.collect_messages(), &[SUBSCRIBER_BEFORE_PAYLOAD]);
    subscriber.finish();
    publisher.finish();
}

#[test]
fn subscriber_after_publisher_receives_from_existing_stream() {
    let daemon = common::DaemonProcess::start("cross-process-subscriber-after");
    let mut publisher = PublisherProcess::spawn(
        &daemon.domain,
        SUBSCRIBER_AFTER_TOPIC,
        SUBSCRIBER_AFTER_PAYLOAD,
    );
    let mut subscriber =
        SubscriberProcess::spawn(&daemon.domain, SUBSCRIBER_AFTER_TOPIC, SINGLE_MESSAGE_COUNT);

    publisher.publish();
    assert_ordered_messages(subscriber.collect_messages(), &[SUBSCRIBER_AFTER_PAYLOAD]);
    subscriber.finish();
    publisher.finish();
}

#[test]
fn several_subscribers_receive_same_publisher_message() {
    let daemon = common::DaemonProcess::start("cross-process-multi-subscriber");
    let mut subscribers = Vec::with_capacity(MULTI_SUBSCRIBER_COUNT);
    for _ in 0..MULTI_SUBSCRIBER_COUNT {
        subscribers.push(SubscriberProcess::spawn(
            &daemon.domain,
            MULTI_SUBSCRIBER_TOPIC,
            SINGLE_MESSAGE_COUNT,
        ));
    }
    let mut publisher = PublisherProcess::spawn(
        &daemon.domain,
        MULTI_SUBSCRIBER_TOPIC,
        MULTI_SUBSCRIBER_PAYLOAD,
    );

    publisher.publish();
    for subscriber in &mut subscribers {
        assert_ordered_messages(subscriber.collect_messages(), &[MULTI_SUBSCRIBER_PAYLOAD]);
    }
    for subscriber in subscribers {
        subscriber.finish();
    }
    publisher.finish();
}

#[test]
fn several_publishers_on_one_topic_are_read_by_one_subscriber() {
    let daemon = common::DaemonProcess::start("cross-process-multi-publisher");
    let mut subscriber = SubscriberProcess::spawn(
        &daemon.domain,
        MULTI_PUBLISHER_TOPIC,
        MULTI_PUBLISHER_MESSAGE_COUNT,
    );
    let mut first_publisher = PublisherProcess::spawn(
        &daemon.domain,
        MULTI_PUBLISHER_TOPIC,
        FIRST_PUBLISHER_PAYLOAD,
    );
    let mut second_publisher = PublisherProcess::spawn(
        &daemon.domain,
        MULTI_PUBLISHER_TOPIC,
        SECOND_PUBLISHER_PAYLOAD,
    );

    first_publisher.publish();
    second_publisher.publish();
    assert_unordered_messages(
        subscriber.collect_messages(),
        &[FIRST_PUBLISHER_PAYLOAD, SECOND_PUBLISHER_PAYLOAD],
    );
    subscriber.finish();
    first_publisher.finish();
    second_publisher.finish();
}

struct PublisherProcess {
    child: ChildProcess,
    stdin: ChildStdin,
}

impl PublisherProcess {
    fn spawn(domain: &str, topic: &str, payload: &str) -> Self {
        let mut child = ChildProcess::spawn(
            CHILD_ROLE_PUBLISHER,
            domain,
            topic,
            payload,
            SINGLE_MESSAGE_COUNT,
            Stdio::piped(),
        );
        let stdin = child.take_stdin();
        child.wait_until_line(PUBLISHER_READY_LINE);
        Self { child, stdin }
    }

    fn publish(&mut self) {
        self.write_command(PUBLISH_COMMAND);
        self.child.wait_until_line(PUBLISHED_LINE);
    }

    fn finish(mut self) {
        self.write_command(EXIT_COMMAND);
        self.child.wait_success(CHILD_ROLE_PUBLISHER);
    }

    fn write_command(&mut self, command: &str) {
        writeln!(self.stdin, "{command}").expect("publisher stdin should accept commands");
        self.stdin
            .flush()
            .expect("publisher operation should succeed");
    }
}

struct SubscriberProcess {
    child: ChildProcess,
    expected_messages: usize,
}

impl SubscriberProcess {
    fn spawn(domain: &str, topic: &str, expected_messages: usize) -> Self {
        let mut child = ChildProcess::spawn(
            CHILD_ROLE_SUBSCRIBER,
            domain,
            topic,
            SUBSCRIBER_UNUSED_PAYLOAD,
            expected_messages,
            Stdio::null(),
        );
        child.wait_until_line(SUBSCRIBER_READY_LINE);
        Self {
            child,
            expected_messages,
        }
    }

    fn collect_messages(&mut self) -> Vec<String> {
        let mut messages = Vec::with_capacity(self.expected_messages);
        while messages.len() < self.expected_messages {
            let line = self.child.read_line();
            if let Some(message) = line.strip_prefix(MESSAGE_PREFIX) {
                messages.push(message.to_owned());
            }
        }
        messages
    }

    fn finish(mut self) {
        self.child.wait_success(CHILD_ROLE_SUBSCRIBER);
    }
}

struct ChildProcess {
    child: Option<Child>,
    stdout: BufReader<ChildStdout>,
}

impl ChildProcess {
    fn spawn(
        role: &str,
        domain: &str,
        topic: &str,
        payload: &str,
        expected_messages: usize,
        stdin: Stdio,
    ) -> Self {
        let mut child = Command::new(
            std::env::current_exe().expect("test executable should be in target/debug/deps"),
        )
        .args([
            "--ignored",
            "--exact",
            "child_cross_process_endpoint",
            "--nocapture",
        ])
        .env(CHILD_ROLE_ENV, role)
        .env(DOMAIN_ENV, domain)
        .env(TOPIC_ENV, topic)
        .env(PAYLOAD_ENV, payload)
        .env(EXPECTED_MESSAGES_ENV, expected_messages.to_string())
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("child process should exit as expected");
        let stdout = child.stdout.take().expect("pipe operation should succeed");
        Self {
            child: Some(child),
            stdout: BufReader::new(stdout),
        }
    }

    fn take_stdin(&mut self) -> ChildStdin {
        self.child
            .as_mut()
            .expect("child process should exit as expected")
            .stdin
            .take()
            .expect("pipe operation should succeed")
    }

    fn wait_until_line(&mut self, expected_line: &str) {
        loop {
            let line = self.read_line();
            if line == expected_line {
                return;
            }
        }
    }

    fn read_line(&mut self) -> String {
        let mut line = String::new();
        let bytes_read = self
            .stdout
            .read_line(&mut line)
            .expect("stdout should contain expected output");
        if bytes_read == EOF_BYTES_READ {
            let output = self
                .child
                .take()
                .expect("child process should exit as expected")
                .wait_with_output()
                .expect("child process should exit as expected");
            panic!(
                "endpoint stdout closed before the expected line: status={} stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        line.trim_end().to_owned()
    }

    fn wait_success(&mut self, label: &str) {
        let output = self
            .child
            .take()
            .expect("child process should exit as expected")
            .wait_with_output()
            .expect("child process should exit as expected");
        assert!(
            output.status.success(),
            "{label} failed: stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

impl Drop for ChildProcess {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn assert_ordered_messages(actual: Vec<String>, expected: &[&str]) {
    let expected_messages = expected_messages(expected);
    assert_eq!(actual, expected_messages);
}

fn assert_unordered_messages(mut actual: Vec<String>, expected: &[&str]) {
    let mut expected_messages = expected_messages(expected);
    actual.sort_unstable();
    expected_messages.sort_unstable();
    assert_eq!(actual, expected_messages);
}

fn expected_messages(expected: &[&str]) -> Vec<String> {
    expected
        .iter()
        .map(|expected_message| (*expected_message).to_owned())
        .collect()
}

#[test]
#[ignore]
fn child_cross_process_endpoint() {
    let Ok(role) = std::env::var(CHILD_ROLE_ENV) else {
        return;
    };
    match role.as_str() {
        CHILD_ROLE_PUBLISHER => run_child_publisher(),
        CHILD_ROLE_SUBSCRIBER => run_child_subscriber(),
        _ => panic!("unknown endpoint role: {role}"),
    }
}

fn run_child_publisher() {
    let domain = std::env::var(DOMAIN_ENV).expect("domain should connect");
    let topic = std::env::var(TOPIC_ENV).expect("operation should succeed");
    let payload = std::env::var(PAYLOAD_ENV).expect("operation should succeed");
    let bus = Bus::connect(&domain).expect("publisher operation should succeed");
    let publisher = bus
        .publisher(&topic, TopicConfig::new(TEST_MAX_MESSAGE_SIZE, TEST_DEPTH))
        .expect("publisher operation should succeed");
    print_child_line(PUBLISHER_READY_LINE);
    expect_stdin_command(PUBLISH_COMMAND);
    publisher
        .publish_bytes(payload.as_bytes())
        .expect("publish should succeed");
    print_child_line(PUBLISHED_LINE);
    expect_stdin_command(EXIT_COMMAND);
}

fn run_child_subscriber() {
    let domain = std::env::var(DOMAIN_ENV).expect("domain should connect");
    let topic = std::env::var(TOPIC_ENV).expect("operation should succeed");
    let expected_messages = std::env::var(EXPECTED_MESSAGES_ENV)
        .expect("message should be available")
        .parse::<usize>()
        .expect("message should be available");
    let bus = Bus::connect(&domain).expect("subscriber operation should succeed");
    let subscriber = bus
        .subscriber(&topic)
        .expect("subscriber_id should be released");
    print_child_line(SUBSCRIBER_READY_LINE);
    for _ in 0..expected_messages {
        let message = subscriber
            .recv_timeout(CHILD_RECEIVE_TIMEOUT)
            .expect("subscriber operation should succeed")
            .expect("subscriber operation should succeed");
        let payload = std::str::from_utf8(message.as_slice()).expect("value should be utf8");
        print_child_line(&format!("{MESSAGE_PREFIX}{payload}"));
    }
}

fn expect_stdin_command(expected_command: &str) {
    let mut line = String::new();
    let bytes_read = std::io::stdin()
        .lock()
        .read_line(&mut line)
        .expect("child process should exit as expected");
    assert_ne!(
        bytes_read, EOF_BYTES_READ,
        "child process should exit as expected"
    );
    assert_eq!(line.trim_end(), expected_command);
}

fn print_child_line(line: &str) {
    println!("{line}");
    std::io::stdout()
        .flush()
        .expect("stdout should contain expected output");
}
