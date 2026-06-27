use rbus::Bus;
use rbus_examples::{ExampleError, ExampleResult, parse_arguments, print_line, run_example};

const USAGE: &str = "multi_sub --domain <domain> --topic <topic> --expected-messages <count> --timeout-ms <milliseconds>";
const DOMAIN_ARGUMENT: &str = "--domain";
const TOPIC_ARGUMENT: &str = "--topic";
const EXPECTED_MESSAGES_ARGUMENT: &str = "--expected-messages";
const TIMEOUT_MILLISECONDS_ARGUMENT: &str = "--timeout-ms";
const ALLOWED_ARGUMENTS: &[&str] = &[
    DOMAIN_ARGUMENT,
    TOPIC_ARGUMENT,
    EXPECTED_MESSAGES_ARGUMENT,
    TIMEOUT_MILLISECONDS_ARGUMENT,
];

fn main() -> std::process::ExitCode {
    run_example(run)
}

fn run() -> ExampleResult<()> {
    let arguments = parse_arguments(USAGE, ALLOWED_ARGUMENTS)?;
    let domain = arguments.required_string(DOMAIN_ARGUMENT)?;
    let topic = arguments.required_string(TOPIC_ARGUMENT)?;
    let expected_messages = arguments.required_usize(EXPECTED_MESSAGES_ARGUMENT)?;
    let receive_timeout = arguments.required_duration_millis(TIMEOUT_MILLISECONDS_ARGUMENT)?;

    let bus = Bus::connect(&domain)?;
    let subscriber = bus.subscriber(&topic)?;
    print_line("multi-subscriber-ready")?;
    for message_index in 0..expected_messages {
        let message = subscriber.recv_timeout(receive_timeout)?.ok_or_else(|| {
            ExampleError::message(format!(
                "receive timeout expired before message index {message_index}"
            ))
        })?;
        let payload = std::str::from_utf8(message.as_slice())?;
        print_line(&format!(
            "multi-received index={message_index} sequence={} bytes={} payload={payload}",
            message.sequence().as_u64(),
            message.as_slice().len()
        ))?;
    }
    Ok(())
}
