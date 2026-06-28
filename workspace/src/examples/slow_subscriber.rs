use std::thread;

use rbus::{Bus, TopicConfig};
use rbus_examples::{ExampleError, ExampleResult, parse_arguments, print_line, run_example};

const USAGE: &str = "slow_subscriber --domain <domain> --topic <topic> --messages <count> --payload-prefix <text> --max-message-size <bytes> --depth <power-of-two> --pause-ms <milliseconds> --timeout-ms <milliseconds>";
const DOMAIN_ARGUMENT: &str = "--domain";
const TOPIC_ARGUMENT: &str = "--topic";
const MESSAGES_ARGUMENT: &str = "--messages";
const PAYLOAD_PREFIX_ARGUMENT: &str = "--payload-prefix";
const MAX_MESSAGE_SIZE_ARGUMENT: &str = "--max-message-size";
const DEPTH_ARGUMENT: &str = "--depth";
const PAUSE_MILLISECONDS_ARGUMENT: &str = "--pause-ms";
const TIMEOUT_MILLISECONDS_ARGUMENT: &str = "--timeout-ms";
const ALLOWED_ARGUMENTS: &[&str] = &[
    DOMAIN_ARGUMENT,
    TOPIC_ARGUMENT,
    MESSAGES_ARGUMENT,
    PAYLOAD_PREFIX_ARGUMENT,
    MAX_MESSAGE_SIZE_ARGUMENT,
    DEPTH_ARGUMENT,
    PAUSE_MILLISECONDS_ARGUMENT,
    TIMEOUT_MILLISECONDS_ARGUMENT,
];

fn main() -> std::process::ExitCode {
    run_example(run)
}

fn run() -> ExampleResult<()> {
    let arguments = parse_arguments(USAGE, ALLOWED_ARGUMENTS)?;
    let domain = arguments.required_string(DOMAIN_ARGUMENT)?;
    let topic = arguments.required_string(TOPIC_ARGUMENT)?;
    let message_count = arguments.required_usize(MESSAGES_ARGUMENT)?;
    let payload_prefix = arguments.required_string(PAYLOAD_PREFIX_ARGUMENT)?;
    let max_message_size = arguments.required_usize(MAX_MESSAGE_SIZE_ARGUMENT)?;
    let depth = arguments.required_usize(DEPTH_ARGUMENT)?;
    let pause_duration = arguments.required_duration_millis(PAUSE_MILLISECONDS_ARGUMENT)?;
    let receive_timeout = arguments.required_duration_millis(TIMEOUT_MILLISECONDS_ARGUMENT)?;

    let bus = Bus::connect(&domain)?;
    let subscriber = bus.subscriber(&topic)?;
    let mut publisher = bus.publisher(&topic, TopicConfig::new(max_message_size, depth))?;
    print_line("slow-subscriber-ready")?;
    for message_index in 0..message_count {
        let payload = format!("{payload_prefix}{message_index}");
        publisher.publish_bytes(payload.as_bytes())?;
        let message = subscriber.recv_timeout(receive_timeout)?.ok_or_else(|| {
            ExampleError::message(format!(
                "receive timeout expired before slow message index {message_index}"
            ))
        })?;
        let received_payload = std::str::from_utf8(message.as_slice())?;
        print_line(&format!(
            "slow-received index={message_index} sequence={} payload={received_payload}",
            message.sequence().as_u64()
        ))?;
        thread::sleep(pause_duration);
    }
    Ok(())
}
