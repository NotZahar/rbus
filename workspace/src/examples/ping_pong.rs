use rbus::{Bus, TopicConfig};
use rbus_examples::{ExampleError, ExampleResult, parse_arguments, print_line, run_example};

const USAGE: &str = "ping_pong --domain <domain> --role <client|server> --ping-topic <topic> --pong-topic <topic> --payload <text> --max-message-size <bytes> --depth <power-of-two> --timeout-ms <milliseconds>";
const CLIENT_ROLE: &str = "client";
const SERVER_ROLE: &str = "server";
const DOMAIN_ARGUMENT: &str = "--domain";
const ROLE_ARGUMENT: &str = "--role";
const PING_TOPIC_ARGUMENT: &str = "--ping-topic";
const PONG_TOPIC_ARGUMENT: &str = "--pong-topic";
const PAYLOAD_ARGUMENT: &str = "--payload";
const MAX_MESSAGE_SIZE_ARGUMENT: &str = "--max-message-size";
const DEPTH_ARGUMENT: &str = "--depth";
const TIMEOUT_MILLISECONDS_ARGUMENT: &str = "--timeout-ms";
const ALLOWED_ARGUMENTS: &[&str] = &[
    DOMAIN_ARGUMENT,
    ROLE_ARGUMENT,
    PING_TOPIC_ARGUMENT,
    PONG_TOPIC_ARGUMENT,
    PAYLOAD_ARGUMENT,
    MAX_MESSAGE_SIZE_ARGUMENT,
    DEPTH_ARGUMENT,
    TIMEOUT_MILLISECONDS_ARGUMENT,
];

fn main() -> std::process::ExitCode {
    run_example(run)
}

fn run() -> ExampleResult<()> {
    let arguments = parse_arguments(USAGE, ALLOWED_ARGUMENTS)?;
    let domain = arguments.required_string(DOMAIN_ARGUMENT)?;
    let role = arguments.required_string(ROLE_ARGUMENT)?;
    let ping_topic = arguments.required_string(PING_TOPIC_ARGUMENT)?;
    let pong_topic = arguments.required_string(PONG_TOPIC_ARGUMENT)?;
    let max_message_size = arguments.required_usize(MAX_MESSAGE_SIZE_ARGUMENT)?;
    let depth = arguments.required_usize(DEPTH_ARGUMENT)?;
    let receive_timeout = arguments.required_duration_millis(TIMEOUT_MILLISECONDS_ARGUMENT)?;

    match role.as_str() {
        CLIENT_ROLE => run_client(
            &domain,
            &ping_topic,
            &pong_topic,
            arguments.required_string(PAYLOAD_ARGUMENT)?,
            max_message_size,
            depth,
            receive_timeout,
        ),
        SERVER_ROLE => run_server(
            &domain,
            &ping_topic,
            &pong_topic,
            max_message_size,
            depth,
            receive_timeout,
        ),
        _ => Err(ExampleError::message(format!(
            "unknown ping_pong role {role}; expected {CLIENT_ROLE} or {SERVER_ROLE}"
        ))),
    }
}

fn run_client(
    domain: &str,
    ping_topic: &str,
    pong_topic: &str,
    payload: String,
    max_message_size: usize,
    depth: usize,
    receive_timeout: std::time::Duration,
) -> ExampleResult<()> {
    let bus = Bus::connect(domain)?;
    let subscriber = bus.subscriber(pong_topic)?;
    let mut publisher = bus.publisher(ping_topic, TopicConfig::new(max_message_size, depth))?;
    let ping_sequence = publisher.publish_bytes(payload.as_bytes())?;
    print_line(&format!(
        "client-ping sequence={} bytes={}",
        ping_sequence.as_u64(),
        payload.len()
    ))?;
    let message = subscriber
        .recv_timeout(receive_timeout)?
        .ok_or_else(|| ExampleError::message("client timeout expired before pong arrived"))?;
    let response_payload = std::str::from_utf8(message.as_slice())?;
    print_line(&format!(
        "client-pong sequence={} bytes={} payload={response_payload}",
        message.sequence().as_u64(),
        message.as_slice().len()
    ))?;
    Ok(())
}

fn run_server(
    domain: &str,
    ping_topic: &str,
    pong_topic: &str,
    max_message_size: usize,
    depth: usize,
    receive_timeout: std::time::Duration,
) -> ExampleResult<()> {
    let bus = Bus::connect(domain)?;
    let subscriber = bus.subscriber(ping_topic)?;
    let mut publisher = bus.publisher(pong_topic, TopicConfig::new(max_message_size, depth))?;
    print_line("server-ready")?;
    let message = subscriber
        .recv_timeout(receive_timeout)?
        .ok_or_else(|| ExampleError::message("server timeout expired before ping arrived"))?;
    let payload = std::str::from_utf8(message.as_slice())?;
    let pong_sequence = publisher.publish_bytes(message.as_slice())?;
    print_line(&format!(
        "server-pong sequence={} bytes={} payload={payload}",
        pong_sequence.as_u64(),
        message.as_slice().len()
    ))?;
    Ok(())
}
