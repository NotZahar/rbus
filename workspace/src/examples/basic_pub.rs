use std::thread;

use rbus::{Bus, TopicConfig};
use rbus_examples::{ExampleResult, parse_arguments, print_line, run_example};

const USAGE: &str = "basic_pub --domain <domain> --topic <topic> --payload <text> --max-message-size <bytes> --depth <power-of-two> --hold-ms <milliseconds>";
const DOMAIN_ARGUMENT: &str = "--domain";
const TOPIC_ARGUMENT: &str = "--topic";
const PAYLOAD_ARGUMENT: &str = "--payload";
const MAX_MESSAGE_SIZE_ARGUMENT: &str = "--max-message-size";
const DEPTH_ARGUMENT: &str = "--depth";
const HOLD_MILLISECONDS_ARGUMENT: &str = "--hold-ms";
const ALLOWED_ARGUMENTS: &[&str] = &[
    DOMAIN_ARGUMENT,
    TOPIC_ARGUMENT,
    PAYLOAD_ARGUMENT,
    MAX_MESSAGE_SIZE_ARGUMENT,
    DEPTH_ARGUMENT,
    HOLD_MILLISECONDS_ARGUMENT,
];

fn main() -> std::process::ExitCode {
    run_example(run)
}

fn run() -> ExampleResult<()> {
    let arguments = parse_arguments(USAGE, ALLOWED_ARGUMENTS)?;
    let domain = arguments.required_string(DOMAIN_ARGUMENT)?;
    let topic = arguments.required_string(TOPIC_ARGUMENT)?;
    let payload = arguments.required_string(PAYLOAD_ARGUMENT)?;
    let max_message_size = arguments.required_usize(MAX_MESSAGE_SIZE_ARGUMENT)?;
    let depth = arguments.required_usize(DEPTH_ARGUMENT)?;
    let hold_duration = arguments.required_duration_millis(HOLD_MILLISECONDS_ARGUMENT)?;

    let bus = Bus::connect(&domain)?;
    let mut publisher = bus.publisher(&topic, TopicConfig::new(max_message_size, depth))?;
    let sequence = publisher.publish_bytes(payload.as_bytes())?;
    print_line(&format!(
        "published sequence={} bytes={}",
        sequence.as_u64(),
        payload.len()
    ))?;
    thread::sleep(hold_duration);
    Ok(())
}
