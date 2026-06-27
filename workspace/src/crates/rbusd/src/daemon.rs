use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use crate::args::DaemonArguments;
use crate::cleanup::{cleanup_domain, format_cleanup_report};
use crate::client_session::ClientSession;
use crate::doctor::{doctor_domain, format_doctor_report};
use crate::domain::domain_socket_path;
use crate::inspect::inspect_domain;
use crate::registry::DaemonRegistry;
use crate::registry_types::NotifyStreamAddedResult;
use crate::signal::should_stop;
use crate::smoke::run_smoke_daemon;
use crate::stats::{daemon_stats_snapshot, format_stats_response};
use crate::warnings::{format_warning_report, warning_report};

use rbus_core::constants::{CONTROL_PROTOCOL_VERSION, MAX_SUBSCRIBERS_PER_STREAM};
use rbus_core::ids::{PublisherId, SubscriberId, TopicHash};
use rbus_core::warning::{Warning, WarningCode};
use rbus_ipc::control::FrameHeader;
use rbus_ipc::control::io::{
    CONTROL_FLAG_JSON, ControlFrame, read_control_frame, write_control_frame_with_file_descriptors,
};
use rbus_ipc::control::messages::{
    AttachStreamsRequest, NOTIFY_STREAM_ADDED_INFINITE_WAIT_NS, NOTIFY_STREAM_ADDED_NO_WAIT_NS,
    NotifyStreamAddedRequest, RegisterPublisherRequest, RegisterSubscriberRequest,
    UnregisterRequest, UnregisterTargetKind,
};
use rbus_ipc::control::opcode::Opcode;
use rbus_ipc::unix_socket::bind_unix_listener;

const MAX_CONTROL_REQUEST_PAYLOAD: usize = 4096;
const MAX_CONTROL_RESPONSE_PAYLOAD: usize = 64 * 1024;
const FIRST_SESSION_ID: u64 = 1;
const RBUSD_FATAL_EXIT_CODE: i32 = 2;

type SharedRegistry = Arc<RegistryState>;

struct RegistryState {
    registry: Mutex<DaemonRegistry>,
    stream_added: Condvar,
}

/// Daemon runtime metadata
pub fn run_daemon(daemon_arguments: DaemonArguments) -> ExitCode {
    let result = if daemon_arguments.smoke {
        run_smoke_daemon(&daemon_arguments)
    } else {
        serve_daemon(&daemon_arguments)
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(warning) => {
            eprintln!(
                "warning [{}]: {}\nstacktrace:\n{}",
                warning.code().as_str(),
                warning.message(),
                std::backtrace::Backtrace::force_capture()
            );
            ExitCode::from(2)
        }
    }
}

fn serve_daemon(daemon_arguments: &DaemonArguments) -> Result<(), Warning> {
    let socket_path = domain_socket_path(&daemon_arguments.domain);
    let listener = bind_unix_listener(&socket_path)?;
    let registry = Arc::new(RegistryState {
        registry: Mutex::new(DaemonRegistry::new()),
        stream_added: Condvar::new(),
    });
    let next_session_id = Arc::new(AtomicU64::new(FIRST_SESSION_ID));
    println!(
        "rbusd listening: domain={} socket={}",
        daemon_arguments.domain,
        socket_path.display()
    );

    for stream_result in listener.incoming() {
        let stream = stream_result.map_err(|error| match error.raw_os_error() {
            Some(errno) => {
                Warning::new(WarningCode::SyscallFailed, "rbusd", "serve_daemon::accept")
                    .with_os_errno(errno)
            }
            None => Warning::new(WarningCode::SyscallFailed, "rbusd", "serve_daemon::accept"),
        })?;
        let session_id = next_session_id.fetch_add(1, Ordering::Relaxed);
        let session_arguments = daemon_arguments.clone();
        let session_registry = Arc::clone(&registry);
        thread::Builder::new()
            .spawn(move || {
                if let Err(warning) =
                    serve_client_session(session_arguments, session_registry, stream, session_id)
                {
                    eprintln!(
                        "warning [{}]: {}\nstacktrace:\n{}",
                        warning.code().as_str(),
                        warning.message(),
                        std::backtrace::Backtrace::force_capture()
                    );
                    std::process::exit(RBUSD_FATAL_EXIT_CODE);
                }
            })
            .map_err(|error| match error.raw_os_error() {
                Some(errno) => {
                    Warning::new(WarningCode::ResourceLimit, "rbusd", "serve_daemon::spawn")
                        .with_os_errno(errno)
                }
                None => Warning::new(WarningCode::ResourceLimit, "rbusd", "serve_daemon::spawn"),
            })?;

        if should_stop() {
            break;
        }
    }
    Ok(())
}

fn serve_client_session(
    daemon_arguments: DaemonArguments,
    registry: SharedRegistry,
    mut stream: std::os::unix::net::UnixStream,
    session_id: u64,
) -> Result<(), Warning> {
    let client_session = ClientSession { session_id };
    loop {
        let request = match read_control_frame(&mut stream, MAX_CONTROL_REQUEST_PAYLOAD) {
            Ok(request) => request,
            Err(warning) if warning.code() == WarningCode::Disconnected => {
                cleanup_client_session(&registry, client_session.session_id)?;
                return Ok(());
            }
            Err(warning) => return Err(warning),
        };
        let request_opcode = request.header.opcode;
        let response = if request_opcode == Opcode::NotifyStreamAdded {
            notify_stream_added_response(&registry, &request.payload)?
        } else {
            let mut registry_guard = lock_registry(&registry, "serve_client_session::lock")?;
            handle_control_request(
                &daemon_arguments,
                &mut registry_guard,
                client_session.session_id,
                request,
            )?
        };
        if request_opcode == Opcode::RegisterPublisher {
            registry.stream_added.notify_all();
        }
        match write_daemon_response(&mut stream, response) {
            Ok(()) => {}
            Err(warning) if warning.code() == WarningCode::Disconnected => {
                cleanup_client_session(&registry, client_session.session_id)?;
                return Ok(());
            }
            Err(warning) => return Err(warning),
        }

        if should_stop() {
            cleanup_client_session(&registry, client_session.session_id)?;
            return Ok(());
        }
    }
}

fn write_daemon_response(
    stream: &mut std::os::unix::net::UnixStream,
    response: DaemonResponse,
) -> Result<(), Warning> {
    if response.frame.payload.len() > MAX_CONTROL_RESPONSE_PAYLOAD {
        return Err(Warning::new(
            WarningCode::ResourceLimit,
            "rbusd",
            "write_daemon_response::response_len",
        ));
    }
    write_control_frame_with_file_descriptors(
        stream,
        response.frame.header,
        &response.frame.payload,
        &response.file_descriptors,
    )
}

fn cleanup_client_session(registry: &SharedRegistry, session_id: u64) -> Result<(), Warning> {
    let mut registry_guard = lock_registry(registry, "cleanup_client_session::lock")?;
    let _removed_count = registry_guard.cleanup_session(session_id)?;
    Ok(())
}

fn lock_registry<'a>(
    registry: &'a SharedRegistry,
    operation: &'static str,
) -> Result<MutexGuard<'a, DaemonRegistry>, Warning> {
    registry
        .registry
        .lock()
        .map_err(|_| Warning::new(WarningCode::InternalInvariantWarning, "rbusd", operation))
}

fn handle_control_request(
    daemon_arguments: &DaemonArguments,
    registry: &mut DaemonRegistry,
    session_id: u64,
    request: ControlFrame,
) -> Result<DaemonResponse, Warning> {
    let json = request.header.flags & CONTROL_FLAG_JSON != 0;
    let frame = match request.header.opcode {
        Opcode::Hello => ControlFrame {
            header: FrameHeader::new(Opcode::Hello, CONTROL_PROTOCOL_VERSION, 0),
            payload: Vec::new(),
        },
        Opcode::RegisterPublisher => {
            return register_publisher_response(registry, session_id, &request.payload);
        }
        Opcode::RegisterSubscriber => {
            return register_subscriber_response(registry, session_id, &request.payload);
        }
        Opcode::AttachStreams => {
            return attach_streams_response(registry, &request.payload);
        }
        Opcode::NotifyStreamAdded => {
            return Err(Warning::new(
                WarningCode::InternalInvariantWarning,
                "rbusd",
                "handle_control_request::notify_stream_added",
            ));
        }
        Opcode::Unregister => {
            return unregister_response(registry, &request.payload);
        }
        Opcode::Inspect => {
            inspect_response(registry, &daemon_arguments.domain, json, &request.payload)?
        }
        Opcode::GetStats => stats_response(registry, &daemon_arguments.domain, json)?,
        Opcode::GetWarnings => warnings_response(registry, &daemon_arguments.domain, json)?,
        Opcode::Cleanup => cleanup_response(registry, &daemon_arguments.domain, json)?,
        Opcode::Doctor => doctor_response(registry, &daemon_arguments.domain, json)?,
    };
    Ok(DaemonResponse {
        frame,
        file_descriptors: Vec::new(),
    })
}

fn register_publisher_response(
    registry: &mut DaemonRegistry,
    session_id: u64,
    payload: &[u8],
) -> Result<DaemonResponse, Warning> {
    let request = RegisterPublisherRequest::decode(payload)?;
    let registered = registry.register_publisher(request, session_id)?;
    let response_payload = registered.response.encode();
    Ok(DaemonResponse {
        frame: ControlFrame {
            header: FrameHeader::new(
                Opcode::RegisterPublisher,
                0,
                control_payload_length(response_payload.len(), "register_publisher_response")?,
            ),
            payload: response_payload.to_vec(),
        },
        file_descriptors: vec![
            registered.file_descriptor,
            registered.topic_wait_file_descriptor,
        ],
    })
}

fn register_subscriber_response(
    registry: &mut DaemonRegistry,
    session_id: u64,
    payload: &[u8],
) -> Result<DaemonResponse, Warning> {
    let request = RegisterSubscriberRequest::decode(payload)?;
    let registered =
        registry.register_subscriber(TopicHash::new(request.topic_hash), session_id)?;
    let response_payload = registered.response.encode();
    Ok(DaemonResponse {
        frame: ControlFrame {
            header: FrameHeader::new(
                Opcode::RegisterSubscriber,
                0,
                control_payload_length(response_payload.len(), "register_subscriber_response")?,
            ),
            payload: response_payload.to_vec(),
        },
        file_descriptors: vec![registered.topic_wait_file_descriptor],
    })
}

fn attach_streams_response(
    registry: &DaemonRegistry,
    payload: &[u8],
) -> Result<DaemonResponse, Warning> {
    let request = AttachStreamsRequest::decode(payload)?;
    let attached = registry.attach_streams(TopicHash::new(request.topic_hash));
    let response_payload = attached.response.encode()?;
    let payload_length = u32::try_from(response_payload.len()).map_err(|_| {
        Warning::new(
            rbus_core::warning::WarningCode::ResourceLimit,
            "rbusd",
            "attach_streams_response::payload_len",
        )
    })?;
    Ok(DaemonResponse {
        frame: ControlFrame {
            header: FrameHeader::new(Opcode::AttachStreams, 0, payload_length),
            payload: response_payload,
        },
        file_descriptors: attached.file_descriptors,
    })
}

fn notify_stream_added_response(
    registry: &SharedRegistry,
    payload: &[u8],
) -> Result<DaemonResponse, Warning> {
    let request = NotifyStreamAddedRequest::decode(payload)?;
    let subscriber_id = subscriber_id_from_u64(
        request.subscriber_id,
        "notify_stream_added_response::subscriber_id",
    )?;
    let registry_guard = lock_registry(registry, "notify_stream_added_response::lock")?;
    let notified = wait_for_notified_streams(
        registry,
        registry_guard,
        TopicHash::new(request.topic_hash),
        subscriber_id,
        request.since_generation,
        request.wait_timeout_ns,
    )?;
    let response_payload = notified.response.encode()?;
    Ok(DaemonResponse {
        frame: ControlFrame {
            header: FrameHeader::new(
                Opcode::NotifyStreamAdded,
                0,
                control_payload_length(response_payload.len(), "notify_stream_added_response")?,
            ),
            payload: response_payload,
        },
        file_descriptors: notified.file_descriptors,
    })
}

fn wait_for_notified_streams(
    registry: &SharedRegistry,
    mut registry_guard: MutexGuard<'_, DaemonRegistry>,
    topic_hash: TopicHash,
    subscriber_id: SubscriberId,
    since_generation: u64,
    wait_timeout_ns: u64,
) -> Result<NotifyStreamAddedResult, Warning> {
    if wait_timeout_ns == NOTIFY_STREAM_ADDED_NO_WAIT_NS {
        return registry_guard.notify_stream_added(topic_hash, subscriber_id, since_generation);
    }

    let deadline = stream_wait_deadline(wait_timeout_ns)?;
    loop {
        let notified =
            registry_guard.notify_stream_added(topic_hash, subscriber_id, since_generation)?;
        if !notified.response.streams.is_empty() {
            return Ok(notified);
        }
        registry_guard = wait_registry_stream_added(registry, registry_guard, deadline)?;
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return registry_guard.notify_stream_added(topic_hash, subscriber_id, since_generation);
        }
    }
}

fn stream_wait_deadline(wait_timeout_ns: u64) -> Result<Option<Instant>, Warning> {
    if wait_timeout_ns == NOTIFY_STREAM_ADDED_INFINITE_WAIT_NS {
        return Ok(None);
    }
    Instant::now()
        .checked_add(Duration::from_nanos(wait_timeout_ns))
        .map(Some)
        .ok_or_else(|| Warning::new(WarningCode::ResourceLimit, "rbusd", "stream_wait_deadline"))
}

fn wait_registry_stream_added<'a>(
    registry: &SharedRegistry,
    registry_guard: MutexGuard<'a, DaemonRegistry>,
    deadline: Option<Instant>,
) -> Result<MutexGuard<'a, DaemonRegistry>, Warning> {
    if let Some(deadline) = deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(registry_guard);
        }
        let (next_guard, _timeout) = registry
            .stream_added
            .wait_timeout(registry_guard, remaining)
            .map_err(|_| Warning::new(WarningCode::InternalInvariantWarning, "rbusd", "wait"))?;
        Ok(next_guard)
    } else {
        registry
            .stream_added
            .wait(registry_guard)
            .map_err(|_| Warning::new(WarningCode::InternalInvariantWarning, "rbusd", "wait"))
    }
}

fn unregister_response(
    registry: &mut DaemonRegistry,
    payload: &[u8],
) -> Result<DaemonResponse, Warning> {
    let request = UnregisterRequest::decode(payload)?;
    let response = match request.target_kind {
        UnregisterTargetKind::Publisher => {
            registry.unregister_publisher(PublisherId::new(request.target_id))?
        }
        UnregisterTargetKind::Subscriber => {
            let subscriber_id =
                subscriber_id_from_u64(request.target_id, "unregister_response::subscriber_id")?;
            registry.unregister_subscriber(subscriber_id)?
        }
    };
    let response_payload = response.encode();
    Ok(DaemonResponse {
        frame: ControlFrame {
            header: FrameHeader::new(
                Opcode::Unregister,
                0,
                control_payload_length(response_payload.len(), "unregister_response")?,
            ),
            payload: response_payload.to_vec(),
        },
        file_descriptors: Vec::new(),
    })
}

fn inspect_response(
    registry: &DaemonRegistry,
    domain: &str,
    json: bool,
    payload: &[u8],
) -> Result<ControlFrame, Warning> {
    let topic = payload_as_str(payload)?;
    let response = inspect_domain(
        registry,
        domain,
        if topic.is_empty() { None } else { Some(topic) },
        json,
    )?;
    response_frame(Opcode::Inspect, response)
}

fn stats_response(
    registry: &DaemonRegistry,
    domain: &str,
    json: bool,
) -> Result<ControlFrame, Warning> {
    let stats = daemon_stats_snapshot(registry)?;
    let response = format_stats_response(domain, &stats, json);
    response_frame(Opcode::GetStats, response)
}

fn warnings_response(
    registry: &DaemonRegistry,
    domain: &str,
    json: bool,
) -> Result<ControlFrame, Warning> {
    let report = warning_report(registry)?;
    let response = format_warning_report(domain, &report, json);
    response_frame(Opcode::GetWarnings, response)
}

fn cleanup_response(
    registry: &mut DaemonRegistry,
    domain: &str,
    json: bool,
) -> Result<ControlFrame, Warning> {
    let cleanup_report = cleanup_domain(registry, domain)?;
    let response = format_cleanup_report(domain, cleanup_report, json)?;
    response_frame(Opcode::Cleanup, response)
}

fn doctor_response(
    registry: &DaemonRegistry,
    domain: &str,
    json: bool,
) -> Result<ControlFrame, Warning> {
    let doctor = doctor_domain(registry, domain)?;
    let response = format_doctor_report(domain, &doctor, json);
    response_frame(Opcode::Doctor, response)
}

fn response_frame(opcode: Opcode, response: String) -> Result<ControlFrame, Warning> {
    let payload = response.into_bytes();
    let payload_length = u32::try_from(payload.len()).map_err(|_| {
        Warning::new(
            rbus_core::warning::WarningCode::ResourceLimit,
            "rbusd",
            "response_frame::payload_len",
        )
    })?;
    Ok(ControlFrame {
        header: FrameHeader::new(opcode, 0, payload_length),
        payload,
    })
}

fn payload_as_str(payload: &[u8]) -> Result<&str, Warning> {
    std::str::from_utf8(payload).map_err(|_| {
        Warning::new(
            rbus_core::warning::WarningCode::ProtocolMismatch,
            "rbusd",
            "payload_as_str",
        )
    })
}

fn control_payload_length(length: usize, operation: &'static str) -> Result<u32, Warning> {
    u32::try_from(length).map_err(|_| {
        Warning::new(
            rbus_core::warning::WarningCode::ResourceLimit,
            "rbusd",
            operation,
        )
    })
}

fn subscriber_id_from_u64(value: u64, operation: &'static str) -> Result<SubscriberId, Warning> {
    if value >= u64::from(MAX_SUBSCRIBERS_PER_STREAM) {
        return Err(Warning::new(
            rbus_core::warning::WarningCode::SubscriberLimitReached,
            "rbusd",
            operation,
        ));
    }
    Ok(SubscriberId::new(value as u8))
}

struct DaemonResponse {
    frame: ControlFrame,
    file_descriptors: Vec<std::os::fd::RawFd>,
}
