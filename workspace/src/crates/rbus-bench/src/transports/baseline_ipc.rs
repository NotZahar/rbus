use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

use rbus_core::warning::{Warning, WarningCode};

use crate::allocation;
use crate::system_metrics::{ProcessSystemSnapshot, monotonic_time_ns};
use crate::transport_kind::TransportKind;
use crate::transports::baseline_paths::{
    make_fifo, pair_path, queue_name, remove_path, stream_path,
};
use crate::transports::posix_queue::PosixMessageQueue;
use crate::transports::tcp_loopback::bind_reusable_listener;
use crate::worker::{
    LATENCY_PREFIX, WorkerArguments, WorkerReport, WorkerRole, warmup_messages_per_publisher,
};

const COMPONENT_RBUS_BENCH: &str = "rbus-bench";
const BASELINE_READY_BYTES: &[u8] = b"ready\n";
const FILL_BYTE: u8 = 0xA5;
const CONNECT_RETRY_ATTEMPTS: usize = 2_000;
const CONNECT_RETRY_INTERVAL_MS: u64 = 1;
const RECV_TIMEOUT_MS: u64 = 5_000;
const TCP_PORT_FILE_EXTENSION: &str = "tcp-port";
const TIMESTAMP_BYTES: usize = core::mem::size_of::<u64>();

/// Benchmark metadata
pub fn run_worker(arguments: &WorkerArguments) -> Result<Option<WorkerReport>, Warning> {
    if arguments.transport.is_rbus() {
        return Ok(None);
    }
    let report = match arguments.role {
        WorkerRole::Publisher => run_publisher(arguments)?,
        WorkerRole::Subscriber => run_subscriber(arguments)?,
        WorkerRole::PingResponder => run_ping_responder(arguments)?,
        WorkerRole::PingRequester => run_ping_requester(arguments)?,
        WorkerRole::CrashSubscriber | WorkerRole::CrashPublisher => return Err(invalid_argument()),
    };
    Ok(Some(report))
}

fn run_publisher(arguments: &WorkerArguments) -> Result<WorkerReport, Warning> {
    let mut sender = BaselineSender::connect(arguments, &arguments.topic)?;
    let mut payload = vec![FILL_BYTE; arguments.payload_size];
    for _message_index in 0..warmup_messages_per_publisher() {
        send_timestamped(&mut sender, &mut payload)?;
    }
    measure_report(u64::from(arguments.messages_per_publisher), || {
        for _message_index in 0..arguments.messages_per_publisher {
            send_timestamped(&mut sender, &mut payload)?;
        }
        Ok(())
    })
}

fn run_subscriber(arguments: &WorkerArguments) -> Result<WorkerReport, Warning> {
    let mut receiver = BaselineReceiver::bind(arguments, &arguments.topic)?;
    write_ready_file(arguments)?;
    receiver.accept(arguments.publisher_count)?;
    let mut payload = vec![0_u8; arguments.payload_size];
    let warmup_messages =
        u64::from(arguments.publisher_count) * u64::from(warmup_messages_per_publisher());
    for _message_index in 0..warmup_messages {
        receiver.receive(&mut payload)?;
    }
    let measured_messages =
        u64::from(arguments.publisher_count) * u64::from(arguments.messages_per_publisher);
    let mut latencies = vec![0_u64; usize_from_u64(measured_messages)?];
    let report = measure_report(measured_messages, || {
        for latency in &mut latencies {
            receiver.receive(&mut payload)?;
            *latency = monotonic_time_ns()?.saturating_sub(decode_timestamp(&payload)?);
            if arguments.slow_subscriber {
                std::thread::sleep(Duration::from_micros(RECV_TIMEOUT_MS));
            }
        }
        Ok(())
    })?;
    for latency in latencies {
        println!("{LATENCY_PREFIX}{latency}");
    }
    Ok(report)
}

fn run_ping_responder(arguments: &WorkerArguments) -> Result<WorkerReport, Warning> {
    let mut receiver = BaselineReceiver::bind(arguments, &arguments.topic)?;
    write_ready_file(arguments)?;
    receiver.accept(1)?;
    let mut sender =
        BaselineSender::connect_to_subscriber(arguments, &arguments.response_topic, 0)?;
    let mut payload = vec![0_u8; arguments.payload_size];
    measure_report(u64::from(arguments.messages_per_publisher), || {
        for _message_index in 0..arguments.messages_per_publisher {
            receiver.receive(&mut payload)?;
            let timestamp_ns = decode_timestamp(&payload)?;
            encode_timestamp(&mut payload, timestamp_ns)?;
            sender.send(&payload)?;
        }
        Ok(())
    })
}

fn run_ping_requester(arguments: &WorkerArguments) -> Result<WorkerReport, Warning> {
    let mut receiver = BaselineReceiver::bind(arguments, &arguments.response_topic)?;
    let mut sender = BaselineSender::connect_to_subscriber(arguments, &arguments.topic, 0)?;
    receiver.accept(1)?;
    let mut payload = vec![FILL_BYTE; arguments.payload_size];
    let mut latencies = vec![0_u64; arguments.messages_per_publisher as usize];
    let report = measure_report(u64::from(arguments.messages_per_publisher), || {
        for latency in &mut latencies {
            send_timestamped(&mut sender, &mut payload)?;
            receiver.receive(&mut payload)?;
            *latency = monotonic_time_ns()?.saturating_sub(decode_timestamp(&payload)?);
        }
        Ok(())
    })?;
    for latency in latencies {
        println!("{LATENCY_PREFIX}{latency}");
    }
    Ok(report)
}

fn measure_report(
    messages: u64,
    operation: impl FnOnce() -> Result<(), Warning>,
) -> Result<WorkerReport, Warning> {
    let system_start = ProcessSystemSnapshot::capture()?;
    let allocation_start = allocation::allocation_count();
    let started_at = monotonic_time_ns()?;
    operation()?;
    let duration_ns = monotonic_time_ns()?.saturating_sub(started_at);
    let allocations = allocation::allocation_delta(allocation_start);
    let system_delta = ProcessSystemSnapshot::capture()?.delta(system_start);
    Ok(WorkerReport {
        messages,
        duration_ns,
        allocations,
        cpu_time_ns: system_delta.cpu_time_ns,
        context_switches: system_delta.context_switches,
        rss_bytes: system_delta.rss_bytes,
        pss_bytes: system_delta.pss_bytes,
        warnings: 0,
        skipped_messages: 0,
        backpressure_pinned_slot: 0,
    })
}

enum BaselineReceiver {
    UnixSocket {
        listener: Option<UnixListener>,
        streams: Vec<UnixStream>,
        next_stream_index: usize,
        path: PathBuf,
    },
    Pipe {
        files: Vec<File>,
        next_file_index: usize,
        paths: Vec<PathBuf>,
    },
    TcpLoopback {
        listener: Option<TcpListener>,
        streams: Vec<TcpStream>,
        next_stream_index: usize,
        port_path: PathBuf,
    },
    PosixMessageQueue {
        queues: Vec<PosixMessageQueue>,
        next_queue_index: usize,
    },
}

impl BaselineReceiver {
    fn bind(arguments: &WorkerArguments, topic: &str) -> Result<Self, Warning> {
        match arguments.transport {
            TransportKind::UnixSocket => bind_unix_socket(arguments, topic),
            TransportKind::Pipe => bind_pipe(arguments, topic),
            TransportKind::TcpLoopback => bind_tcp_loopback(arguments, topic),
            TransportKind::PosixMessageQueue => bind_posix_message_queue(arguments, topic),
            _ => Err(invalid_argument()),
        }
    }

    fn accept(&mut self, expected_streams: u8) -> Result<(), Warning> {
        match self {
            Self::UnixSocket {
                listener,
                streams,
                path,
                ..
            } => {
                let listener = listener.take().ok_or_else(invalid_argument)?;
                for _stream_index in 0..expected_streams {
                    let (stream, _address) = listener.accept().map_err(warning_from_io)?;
                    streams.push(stream);
                }
                remove_path(path)?;
            }
            Self::TcpLoopback {
                listener, streams, ..
            } => {
                let listener = listener.take().ok_or_else(invalid_argument)?;
                for _stream_index in 0..expected_streams {
                    let (stream, _address) = listener.accept().map_err(warning_from_io)?;
                    streams.push(stream);
                }
            }
            Self::Pipe { .. } | Self::PosixMessageQueue { .. } => {}
        }
        Ok(())
    }

    fn receive(&mut self, payload: &mut [u8]) -> Result<(), Warning> {
        match self {
            Self::UnixSocket {
                streams,
                next_stream_index,
                ..
            } => receive_stream(streams, next_stream_index, payload),
            Self::Pipe {
                files,
                next_file_index,
                ..
            } => receive_stream(files, next_file_index, payload),
            Self::TcpLoopback {
                streams,
                next_stream_index,
                ..
            } => receive_stream(streams, next_stream_index, payload),
            Self::PosixMessageQueue {
                queues,
                next_queue_index,
            } => receive_queue(queues, next_queue_index, payload),
        }
    }
}

impl Drop for BaselineReceiver {
    fn drop(&mut self) {
        match self {
            Self::UnixSocket { path, .. } => {
                let _ = std::fs::remove_file(path);
            }
            Self::Pipe { paths, .. } => {
                for path in paths {
                    let _ = std::fs::remove_file(path);
                }
            }
            Self::TcpLoopback { port_path, .. } => {
                let _ = std::fs::remove_file(port_path);
            }
            Self::PosixMessageQueue { .. } => {}
        }
    }
}

enum BaselineSender {
    UnixSocket(Vec<UnixStream>),
    Pipe(Vec<File>),
    TcpLoopback(Vec<TcpStream>),
    PosixMessageQueue(Vec<PosixMessageQueue>),
}

impl BaselineSender {
    fn connect(arguments: &WorkerArguments, topic: &str) -> Result<Self, Warning> {
        match arguments.transport {
            TransportKind::UnixSocket => connect_unix_socket(arguments, topic),
            TransportKind::Pipe => connect_pipe(arguments, topic),
            TransportKind::TcpLoopback => connect_tcp_loopback(arguments, topic),
            TransportKind::PosixMessageQueue => connect_posix_message_queue(arguments, topic),
            _ => Err(invalid_argument()),
        }
    }

    fn connect_to_subscriber(
        arguments: &WorkerArguments,
        topic: &str,
        subscriber_index: u8,
    ) -> Result<Self, Warning> {
        let mut single_subscriber_arguments = arguments.clone();
        single_subscriber_arguments.subscriber_count = 1;
        single_subscriber_arguments.worker_index = subscriber_index;
        Self::connect(&single_subscriber_arguments, topic)
    }

    fn send(&mut self, payload: &[u8]) -> Result<(), Warning> {
        match self {
            Self::UnixSocket(streams) => send_streams(streams, payload),
            Self::Pipe(files) => send_streams(files, payload),
            Self::TcpLoopback(streams) => send_streams(streams, payload),
            Self::PosixMessageQueue(queues) => {
                for queue in queues {
                    queue.send(payload)?;
                }
                Ok(())
            }
        }
    }
}

fn bind_unix_socket(arguments: &WorkerArguments, topic: &str) -> Result<BaselineReceiver, Warning> {
    let path = stream_path(arguments, topic, "sock", arguments.worker_index);
    remove_path(&path)?;
    let listener = UnixListener::bind(&path).map_err(warning_from_io)?;
    Ok(BaselineReceiver::UnixSocket {
        listener: Some(listener),
        streams: Vec::new(),
        next_stream_index: 0,
        path,
    })
}

fn connect_unix_socket(
    arguments: &WorkerArguments,
    topic: &str,
) -> Result<BaselineSender, Warning> {
    let mut streams = Vec::new();
    for subscriber_index in 0..arguments.subscriber_count {
        let path = stream_path(arguments, topic, "sock", subscriber_index);
        streams.push(retry_connect(|| {
            UnixStream::connect(&path).map_err(warning_from_io)
        })?);
    }
    Ok(BaselineSender::UnixSocket(streams))
}

fn bind_pipe(arguments: &WorkerArguments, topic: &str) -> Result<BaselineReceiver, Warning> {
    let mut files = Vec::new();
    let mut paths = Vec::new();
    for publisher_index in 0..arguments.publisher_count {
        let path = pair_path(
            arguments,
            topic,
            "fifo",
            publisher_index,
            arguments.worker_index,
        );
        remove_path(&path)?;
        make_fifo(&path)?;
        files.push(
            OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_CLOEXEC)
                .open(&path)
                .map_err(warning_from_io)?,
        );
        paths.push(path);
    }
    Ok(BaselineReceiver::Pipe {
        files,
        next_file_index: 0,
        paths,
    })
}

fn connect_pipe(arguments: &WorkerArguments, topic: &str) -> Result<BaselineSender, Warning> {
    let mut files = Vec::new();
    for subscriber_index in 0..arguments.subscriber_count {
        let path = pair_path(
            arguments,
            topic,
            "fifo",
            arguments.worker_index,
            subscriber_index,
        );
        files.push(retry_connect(|| {
            OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_CLOEXEC)
                .open(&path)
                .map_err(warning_from_io)
        })?);
    }
    Ok(BaselineSender::Pipe(files))
}

fn bind_tcp_loopback(
    arguments: &WorkerArguments,
    topic: &str,
) -> Result<BaselineReceiver, Warning> {
    let listener = bind_reusable_listener(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
    let port_path = stream_path(
        arguments,
        topic,
        TCP_PORT_FILE_EXTENSION,
        arguments.worker_index,
    );
    remove_path(&port_path)?;
    std::fs::write(
        &port_path,
        listener
            .local_addr()
            .map_err(warning_from_io)?
            .port()
            .to_string(),
    )
    .map_err(warning_from_io)?;
    Ok(BaselineReceiver::TcpLoopback {
        listener: Some(listener),
        streams: Vec::new(),
        next_stream_index: 0,
        port_path,
    })
}

fn connect_tcp_loopback(
    arguments: &WorkerArguments,
    topic: &str,
) -> Result<BaselineSender, Warning> {
    let mut streams = Vec::new();
    for subscriber_index in 0..arguments.subscriber_count {
        let port_path = stream_path(arguments, topic, TCP_PORT_FILE_EXTENSION, subscriber_index);
        streams.push(retry_connect(|| {
            let port = read_tcp_port(&port_path)?;
            let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
            TcpStream::connect(address).map_err(warning_from_io)
        })?);
    }
    Ok(BaselineSender::TcpLoopback(streams))
}

fn read_tcp_port(path: &PathBuf) -> Result<u16, Warning> {
    std::fs::read_to_string(path)
        .map_err(warning_from_io)?
        .trim()
        .parse::<u16>()
        .map_err(|_| invalid_argument())
}

fn bind_posix_message_queue(
    arguments: &WorkerArguments,
    topic: &str,
) -> Result<BaselineReceiver, Warning> {
    let mut queues = Vec::new();
    for publisher_index in 0..arguments.publisher_count {
        let name = queue_name(arguments, topic, publisher_index, arguments.worker_index)?;
        queues.push(PosixMessageQueue::create(name)?);
    }
    Ok(BaselineReceiver::PosixMessageQueue {
        queues,
        next_queue_index: 0,
    })
}

fn connect_posix_message_queue(
    arguments: &WorkerArguments,
    topic: &str,
) -> Result<BaselineSender, Warning> {
    let mut queues = Vec::new();
    for subscriber_index in 0..arguments.subscriber_count {
        let name = queue_name(arguments, topic, arguments.worker_index, subscriber_index)?;
        queues.push(retry_connect(|| PosixMessageQueue::open(name.clone()))?);
    }
    Ok(BaselineSender::PosixMessageQueue(queues))
}

fn send_timestamped(sender: &mut BaselineSender, payload: &mut [u8]) -> Result<(), Warning> {
    payload.fill(FILL_BYTE);
    encode_timestamp(payload, monotonic_time_ns()?)?;
    sender.send(payload)
}

fn send_streams<Stream: Write>(streams: &mut [Stream], payload: &[u8]) -> Result<(), Warning> {
    for stream in streams {
        stream.write_all(payload).map_err(warning_from_io)?;
    }
    Ok(())
}

fn receive_stream<Stream: Read>(
    streams: &mut [Stream],
    next_stream_index: &mut usize,
    payload: &mut [u8],
) -> Result<(), Warning> {
    let stream_index = *next_stream_index % streams.len();
    streams[stream_index]
        .read_exact(payload)
        .map_err(warning_from_io)?;
    *next_stream_index = (*next_stream_index + 1) % streams.len();
    Ok(())
}

fn receive_queue(
    queues: &mut [PosixMessageQueue],
    next_queue_index: &mut usize,
    payload: &mut [u8],
) -> Result<(), Warning> {
    let queue_index = *next_queue_index % queues.len();
    queues[queue_index].receive(payload)?;
    *next_queue_index = (*next_queue_index + 1) % queues.len();
    Ok(())
}

fn retry_connect<Connection>(
    mut operation: impl FnMut() -> Result<Connection, Warning>,
) -> Result<Connection, Warning> {
    let mut last_warning = invalid_argument();
    for _attempt in 0..CONNECT_RETRY_ATTEMPTS {
        match operation() {
            Ok(connection) => return Ok(connection),
            Err(warning) => last_warning = warning,
        }
        std::thread::sleep(Duration::from_millis(CONNECT_RETRY_INTERVAL_MS));
    }
    Err(last_warning)
}

fn encode_timestamp(payload: &mut [u8], timestamp_ns: u64) -> Result<(), Warning> {
    let target = payload
        .get_mut(..TIMESTAMP_BYTES)
        .ok_or_else(invalid_argument)?;
    target.copy_from_slice(&timestamp_ns.to_ne_bytes());
    Ok(())
}

fn decode_timestamp(payload: &[u8]) -> Result<u64, Warning> {
    let source = payload
        .get(..TIMESTAMP_BYTES)
        .ok_or_else(invalid_argument)?;
    let mut bytes = [0_u8; TIMESTAMP_BYTES];
    bytes.copy_from_slice(source);
    Ok(u64::from_ne_bytes(bytes))
}

fn write_ready_file(arguments: &WorkerArguments) -> Result<(), Warning> {
    match &arguments.ready_file {
        Some(path) => std::fs::write(path, BASELINE_READY_BYTES).map_err(warning_from_io),
        None => Ok(()),
    }
}

fn usize_from_u64(value: u64) -> Result<usize, Warning> {
    usize::try_from(value).map_err(|_| invalid_argument())
}

fn warning_from_io(error: std::io::Error) -> Warning {
    match error.raw_os_error() {
        Some(errno) => Warning::new(
            WarningCode::SyscallFailed,
            COMPONENT_RBUS_BENCH,
            "baseline_ipc",
        )
        .with_os_errno(errno),
        None => Warning::new(
            WarningCode::SyscallFailed,
            COMPONENT_RBUS_BENCH,
            "baseline_ipc",
        ),
    }
}

fn invalid_argument() -> Warning {
    Warning::new(
        WarningCode::InvalidConfig,
        COMPONENT_RBUS_BENCH,
        "baseline_ipc",
    )
}
