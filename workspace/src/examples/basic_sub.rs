use rbus::Bus;
use rbus_examples::{ExampleError, ExampleResult, parse_arguments, print_line, run_example};

const USAGE: &str = "basic_sub --domain <domain> --topic <topic> --timeout-ms <milliseconds>";
const DOMAIN_ARGUMENT: &str = "--domain";
const TOPIC_ARGUMENT: &str = "--topic";
const TIMEOUT_MILLISECONDS_ARGUMENT: &str = "--timeout-ms";
const ALLOWED_ARGUMENTS: &[&str] = &[
    DOMAIN_ARGUMENT,
    TOPIC_ARGUMENT,
    TIMEOUT_MILLISECONDS_ARGUMENT,
];

fn main() -> std::process::ExitCode {
    run_example(run)
}

fn run() -> ExampleResult<()> {
    let arguments = parse_arguments(USAGE, ALLOWED_ARGUMENTS)?;
    let domain = arguments.required_string(DOMAIN_ARGUMENT)?;
    let topic = arguments.required_string(TOPIC_ARGUMENT)?;
    let receive_timeout = arguments.required_duration_millis(TIMEOUT_MILLISECONDS_ARGUMENT)?;

    let bus = Bus::connect(&domain)?;
    let subscriber = bus.subscriber(&topic)?;
    print_line("subscriber-ready")?;
    let message = subscriber
        .recv_timeout(receive_timeout)?
        .ok_or_else(|| ExampleError::message("receive timeout expired before a message arrived"))?;
    let payload = std::str::from_utf8(message.as_slice())?;
    print_line(&format!(
        "received sequence={} bytes={} payload={payload}",
        message.sequence().as_u64(),
        message.as_slice().len()
    ))?;
    Ok(())
}
