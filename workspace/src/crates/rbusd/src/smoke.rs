use crate::args::DaemonArguments;
use crate::cleanup::cleanup_domain;
use crate::client_session::ClientSession;
use crate::domain::domain_socket_path;
use crate::inspect::inspect_domain;
use crate::protocol_handler::handle_hello;
use crate::publisher_entry::PublisherEntry;
use crate::registry::DaemonRegistry;
use crate::registry_types::RegistrySnapshot;
use crate::signal::should_stop;
use crate::stats::daemon_stats_snapshot;
use crate::stream_entry::StreamEntry;
use crate::subscriber_entry::SubscriberEntry;
use crate::warnings::warning_report;

use rbus_core::warning::Warning;

pub fn run_smoke_daemon(daemon_arguments: &DaemonArguments) -> Result<(), Warning> {
    let smoke_state = collect_daemon_smoke_state(daemon_arguments)?;
    print_smoke_state(smoke_state);
    Ok(())
}

struct DaemonSmokeState {
    domain: String,
    foreground: bool,
    socket_path: String,
    publisher_count: u64,
    subscriber_count: u64,
    topic_count: u64,
    stream_count: u64,
    session_id: u64,
    publisher_id: u64,
    subscriber_id: u8,
    stream_id: u64,
    hello_opcode: u16,
    protocol_version: u16,
    inspect_summary: String,
    doctor_summary: String,
    warning_summary: String,
    cleanup_count: u64,
    current_sequence: u64,
    should_stop: bool,
}

fn collect_daemon_smoke_state(
    daemon_arguments: &DaemonArguments,
) -> Result<DaemonSmokeState, Warning> {
    let socket_path = domain_socket_path(&daemon_arguments.domain);
    let mut registry = DaemonRegistry::new();
    let snapshot: RegistrySnapshot = registry.snapshot();
    let client_session = ClientSession { session_id: 0 };
    let publisher_entry = PublisherEntry { publisher_id: 0 };
    let subscriber_entry = SubscriberEntry { subscriber_id: 0 };
    let stream_entry = StreamEntry { stream_id: 0 };
    let hello = handle_hello();
    let stats = daemon_stats_snapshot(&registry)?;
    let warnings = warning_report(&registry)?;
    let cleanup = cleanup_domain(&mut registry, &daemon_arguments.domain)?;

    Ok(DaemonSmokeState {
        domain: daemon_arguments.domain.clone(),
        foreground: daemon_arguments.foreground,
        socket_path: socket_path.display().to_string(),
        publisher_count: registry.publisher_count(),
        subscriber_count: registry.subscriber_count(),
        topic_count: snapshot.topic_count,
        stream_count: snapshot.stream_count,
        session_id: client_session.session_id,
        publisher_id: publisher_entry.publisher_id,
        subscriber_id: subscriber_entry.subscriber_id,
        stream_id: stream_entry.stream_id,
        hello_opcode: hello.opcode as u16,
        protocol_version: hello.flags,
        inspect_summary: inspect_domain(&registry, &daemon_arguments.domain, None, false)?,
        doctor_summary: String::from("smoke without bound socket"),
        warning_summary: format!("warnings_total={}", warnings.warnings_total),
        cleanup_count: cleanup.total_removed()?,
        current_sequence: stats.current_sequence,
        should_stop: should_stop(),
    })
}

fn print_smoke_state(smoke_state: DaemonSmokeState) {
    println!(
        concat!(
            "rbusd running: domain={}, foreground={}, socket={}, ",
            "publishers={}, subscribers={}, topics={}, streams={}, ",
            "session={}, publisher={}, subscriber={}, stream={}, ",
            "hello_opcode={}, protocol={}, inspect={}, doctor={}, warnings={}, ",
            "cleanup={}, sequence={}, should_stop={}"
        ),
        smoke_state.domain,
        smoke_state.foreground,
        smoke_state.socket_path,
        smoke_state.publisher_count,
        smoke_state.subscriber_count,
        smoke_state.topic_count,
        smoke_state.stream_count,
        smoke_state.session_id,
        smoke_state.publisher_id,
        smoke_state.subscriber_id,
        smoke_state.stream_id,
        smoke_state.hello_opcode,
        smoke_state.protocol_version,
        smoke_state.inspect_summary,
        smoke_state.doctor_summary,
        smoke_state.warning_summary,
        smoke_state.cleanup_count,
        smoke_state.current_sequence,
        smoke_state.should_stop
    );
}
