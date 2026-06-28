use std::path::{Path, PathBuf};
use std::time::Duration;

use rbus_core::constants::{CONTROL_PROTOCOL_VERSION, MAX_SUBSCRIBERS_PER_STREAM};
use rbus_core::ids::{PublisherId, StreamId, SubscriberId, TopicHash};
use rbus_core::warning::{Warning, WarningCode};
use rbus_ipc::control::client::{
    ControlClient, request_control_frame, request_control_frame_with_file_descriptors,
};
use rbus_ipc::control::messages::{
    AttachStreamsRequest, AttachStreamsResponse, NOTIFY_STREAM_ADDED_INFINITE_WAIT_NS,
    NOTIFY_STREAM_ADDED_NO_WAIT_NS, NotifyStreamAddedRequest, NotifyStreamAddedResponse,
    RegisterPublisherRequest, RegisterPublisherResponse, RegisterSubscriberRequest,
    RegisterSubscriberResponse,
};
use rbus_ipc::control::opcode::Opcode;
use rbus_ipc::control::reregister::{
    ReregisterPublisherRequest, ReregisterPublisherResponse, ReregisterSubscriberRequest,
};
use rbus_ipc::fd::OwnedFileDescriptor;
use rbus_ipc::stream_segment::StreamSegmentMetadata;
use rbus_ipc::unix_socket::default_socket_path;

use crate::config::TopicConfig;

const COMPONENT_RBUS: &str = "rbus";
const MAX_ATTACH_STREAMS_RESPONSE: usize = 8192;
const MAX_ATTACH_STREAMS_PER_RESPONSE: usize = 128;
const REGISTER_PUBLISHER_FD_COUNT: usize = 2;
const REGISTER_SUBSCRIBER_FD_COUNT: usize = 1;
const REREGISTER_RESPONSE_FD_LIMIT: usize = 1;
const REREGISTER_SUBSCRIBER_FD_COUNT: usize = 1;
const STREAM_FD_INDEX: usize = 0;

pub(crate) struct PublisherRegistration {
    pub(crate) metadata: StreamSegmentMetadata,
    pub(crate) stream_generation: u64,
    pub(crate) subscriber_count: u64,
    pub(crate) file_descriptor: OwnedFileDescriptor,
    pub(crate) topic_wait_file_descriptor: OwnedFileDescriptor,
    pub(crate) control_client: ControlClient,
}

pub(crate) struct SubscriberRegistration {
    pub(crate) subscriber_id: SubscriberId,
    pub(crate) subscribed_generation: u64,
    pub(crate) topic_wait_file_descriptor: OwnedFileDescriptor,
    pub(crate) control_client: ControlClient,
}

pub(crate) struct AttachedStream {
    pub(crate) metadata: StreamSegmentMetadata,
    pub(crate) stream_generation: u64,
    pub(crate) file_descriptor: OwnedFileDescriptor,
}

pub(crate) struct StreamNotification {
    pub(crate) latest_generation: u64,
    pub(crate) streams: Vec<AttachedStream>,
}

pub(crate) struct ReregisteredPublisher {
    pub(crate) subscriber_count: u64,
    pub(crate) control_client: ControlClient,
}

pub(crate) fn connect_daemon(domain: &str) -> Result<PathBuf, Warning> {
    let socket_path = default_socket_path(domain);
    let response = request_control_frame(&socket_path, Opcode::Hello, 0, &[], 0)?;
    if response.header.opcode == Opcode::Hello
        && response.header.flags == CONTROL_PROTOCOL_VERSION
        && response.payload.is_empty()
    {
        Ok(socket_path)
    } else {
        Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_RBUS,
            "connect_daemon::hello",
        ))
    }
}

pub(crate) fn register_publisher(
    socket_path: &Path,
    topic_hash: TopicHash,
    config: TopicConfig,
) -> Result<PublisherRegistration, Warning> {
    let payload = RegisterPublisherRequest {
        topic_hash: topic_hash.as_u64(),
        max_message_size: config.max_message_size() as u64,
        depth: config.depth() as u64,
    }
    .encode();
    let mut control_client = ControlClient::connect(socket_path)?;
    let mut response = control_client.request_frame_with_file_descriptors(
        Opcode::RegisterPublisher,
        0,
        &payload,
        RegisterPublisherResponse::ENCODED_SIZE,
        REGISTER_PUBLISHER_FD_COUNT,
    )?;
    if response.frame.header.opcode != Opcode::RegisterPublisher {
        return Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_RBUS,
            "register_publisher::opcode",
        ));
    }
    if response.file_descriptors.len() != REGISTER_PUBLISHER_FD_COUNT {
        return Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_RBUS,
            "register_publisher::fd_count",
        ));
    }
    let decoded = RegisterPublisherResponse::decode(&response.frame.payload)?;
    let file_descriptor = response.file_descriptors.remove(STREAM_FD_INDEX);
    let topic_wait_file_descriptor = response.file_descriptors.remove(STREAM_FD_INDEX);
    Ok(PublisherRegistration {
        metadata: StreamSegmentMetadata {
            stream_id: StreamId::new(decoded.stream_id),
            publisher_id: PublisherId::new(decoded.publisher_id),
            topic_hash,
            segment_size: u64_to_usize(decoded.segment_size, "register_publisher::segment_size")?,
            slot_size: u64_to_usize(decoded.slot_size, "register_publisher::slot_size")?,
            depth: u64_to_usize(decoded.depth, "register_publisher::depth")?,
        },
        stream_generation: decoded.stream_generation,
        subscriber_count: decoded.subscriber_count,
        file_descriptor,
        topic_wait_file_descriptor,
        control_client,
    })
}

pub(crate) fn register_subscriber(
    socket_path: &Path,
    topic_hash: TopicHash,
) -> Result<SubscriberRegistration, Warning> {
    let payload = RegisterSubscriberRequest {
        topic_hash: topic_hash.as_u64(),
    }
    .encode();
    let mut control_client = ControlClient::connect(socket_path)?;
    let mut response = control_client.request_frame_with_file_descriptors(
        Opcode::RegisterSubscriber,
        0,
        &payload,
        RegisterSubscriberResponse::ENCODED_SIZE,
        REGISTER_SUBSCRIBER_FD_COUNT,
    )?;
    if response.frame.header.opcode != Opcode::RegisterSubscriber {
        return Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_RBUS,
            "register_subscriber::opcode",
        ));
    }
    if response.file_descriptors.len() != REGISTER_SUBSCRIBER_FD_COUNT {
        return Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_RBUS,
            "register_subscriber::fd_count",
        ));
    }
    let decoded = RegisterSubscriberResponse::decode(&response.frame.payload)?;
    if decoded.subscriber_id >= u64::from(MAX_SUBSCRIBERS_PER_STREAM) {
        return Err(Warning::new(
            WarningCode::SubscriberLimitReached,
            COMPONENT_RBUS,
            "register_subscriber::subscriber_id",
        ));
    }
    Ok(SubscriberRegistration {
        subscriber_id: SubscriberId::new(decoded.subscriber_id as u8),
        subscribed_generation: decoded.subscribed_generation,
        topic_wait_file_descriptor: response.file_descriptors.remove(STREAM_FD_INDEX),
        control_client,
    })
}

pub(crate) fn reregister_publisher(
    socket_path: &Path,
    metadata: StreamSegmentMetadata,
    stream_file_descriptor: &OwnedFileDescriptor,
    topic_wait_file_descriptor: &OwnedFileDescriptor,
) -> Result<ReregisteredPublisher, Warning> {
    let payload = ReregisterPublisherRequest {
        topic_hash: metadata.topic_hash.as_u64(),
        publisher_id: metadata.publisher_id.as_u64(),
        stream_id: metadata.stream_id.as_u64(),
        segment_size: metadata.segment_size as u64,
        slot_size: metadata.slot_size as u64,
        depth: metadata.depth as u64,
    }
    .encode();
    let mut control_client = ControlClient::connect(socket_path)?;
    let response = control_client.request_frame_with_sent_file_descriptors(
        Opcode::ReregisterPublisher,
        0,
        &payload,
        &[
            stream_file_descriptor.as_raw_file_descriptor(),
            topic_wait_file_descriptor.as_raw_file_descriptor(),
        ],
        ReregisterPublisherResponse::ENCODED_SIZE,
        REREGISTER_RESPONSE_FD_LIMIT,
    )?;
    if response.frame.header.opcode != Opcode::ReregisterPublisher {
        return Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_RBUS,
            "reregister_publisher::opcode",
        ));
    }
    if !response.file_descriptors.is_empty() {
        return Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_RBUS,
            "reregister_publisher::fd_count",
        ));
    }
    let decoded = ReregisterPublisherResponse::decode(&response.frame.payload)?;
    Ok(ReregisteredPublisher {
        subscriber_count: decoded.subscriber_count,
        control_client,
    })
}

pub(crate) fn reregister_subscriber(
    socket_path: &Path,
    topic_hash: TopicHash,
    topic_wait_file_descriptor: &OwnedFileDescriptor,
) -> Result<SubscriberRegistration, Warning> {
    let payload = ReregisterSubscriberRequest {
        topic_hash: topic_hash.as_u64(),
    }
    .encode();
    let mut control_client = ControlClient::connect(socket_path)?;
    let mut response = control_client.request_frame_with_sent_file_descriptors(
        Opcode::ReregisterSubscriber,
        0,
        &payload,
        &[topic_wait_file_descriptor.as_raw_file_descriptor()],
        RegisterSubscriberResponse::ENCODED_SIZE,
        REREGISTER_SUBSCRIBER_FD_COUNT,
    )?;
    if response.frame.header.opcode != Opcode::ReregisterSubscriber {
        return Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_RBUS,
            "reregister_subscriber::opcode",
        ));
    }
    if response.file_descriptors.len() != REREGISTER_SUBSCRIBER_FD_COUNT {
        return Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_RBUS,
            "reregister_subscriber::fd_count",
        ));
    }
    let decoded = RegisterSubscriberResponse::decode(&response.frame.payload)?;
    if decoded.subscriber_id >= u64::from(MAX_SUBSCRIBERS_PER_STREAM) {
        return Err(Warning::new(
            WarningCode::SubscriberLimitReached,
            COMPONENT_RBUS,
            "reregister_subscriber::subscriber_id",
        ));
    }
    Ok(SubscriberRegistration {
        subscriber_id: SubscriberId::new(decoded.subscriber_id as u8),
        subscribed_generation: decoded.subscribed_generation,
        topic_wait_file_descriptor: response.file_descriptors.remove(STREAM_FD_INDEX),
        control_client,
    })
}

pub(crate) fn check_control_session(control_client: &mut ControlClient) -> Result<(), Warning> {
    let response = control_client.request_frame(Opcode::Hello, 0, &[], 0)?;
    if response.header.opcode == Opcode::Hello
        && response.header.flags == CONTROL_PROTOCOL_VERSION
        && response.payload.is_empty()
    {
        Ok(())
    } else {
        Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_RBUS,
            "check_control_session::hello",
        ))
    }
}

pub(crate) fn attach_streams(
    socket_path: &Path,
    topic_hash: TopicHash,
) -> Result<Vec<AttachedStream>, Warning> {
    let payload = AttachStreamsRequest {
        topic_hash: topic_hash.as_u64(),
    }
    .encode();
    let response = request_control_frame_with_file_descriptors(
        socket_path,
        Opcode::AttachStreams,
        0,
        &payload,
        MAX_ATTACH_STREAMS_RESPONSE,
        MAX_ATTACH_STREAMS_PER_RESPONSE,
    )?;
    if response.frame.header.opcode != Opcode::AttachStreams {
        return Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_RBUS,
            "attach_streams::opcode",
        ));
    }
    let decoded = AttachStreamsResponse::decode(&response.frame.payload)?;
    if decoded.streams.len() != response.file_descriptors.len() {
        return Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_RBUS,
            "attach_streams::fd_count",
        ));
    }

    let mut attached_streams = Vec::with_capacity(decoded.streams.len());
    for (metadata, file_descriptor) in decoded
        .streams
        .iter()
        .copied()
        .zip(response.file_descriptors)
    {
        attached_streams.push(AttachedStream {
            metadata: StreamSegmentMetadata {
                stream_id: StreamId::new(metadata.stream_id),
                publisher_id: PublisherId::new(metadata.publisher_id),
                topic_hash,
                segment_size: u64_to_usize(metadata.segment_size, "attach_streams::segment_size")?,
                slot_size: u64_to_usize(metadata.slot_size, "attach_streams::slot_size")?,
                depth: u64_to_usize(metadata.depth, "attach_streams::depth")?,
            },
            stream_generation: metadata.stream_generation,
            file_descriptor,
        });
    }
    Ok(attached_streams)
}

pub(crate) fn notify_stream_added(
    socket_path: &Path,
    topic_hash: TopicHash,
    subscriber_id: SubscriberId,
    since_generation: u64,
) -> Result<StreamNotification, Warning> {
    request_stream_added_notification(
        socket_path,
        topic_hash,
        subscriber_id,
        since_generation,
        NOTIFY_STREAM_ADDED_NO_WAIT_NS,
    )
}

pub(crate) fn wait_stream_added(
    socket_path: &Path,
    topic_hash: TopicHash,
    subscriber_id: SubscriberId,
    since_generation: u64,
    timeout: Option<Duration>,
) -> Result<StreamNotification, Warning> {
    let wait_timeout_ns = match timeout {
        Some(timeout) => u64::try_from(timeout.as_nanos()).map_err(|_| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_RBUS,
                "wait_stream_added::timeout",
            )
        })?,
        None => NOTIFY_STREAM_ADDED_INFINITE_WAIT_NS,
    };
    request_stream_added_notification(
        socket_path,
        topic_hash,
        subscriber_id,
        since_generation,
        wait_timeout_ns,
    )
}

fn request_stream_added_notification(
    socket_path: &Path,
    topic_hash: TopicHash,
    subscriber_id: SubscriberId,
    since_generation: u64,
    wait_timeout_ns: u64,
) -> Result<StreamNotification, Warning> {
    let payload = NotifyStreamAddedRequest {
        topic_hash: topic_hash.as_u64(),
        subscriber_id: u64::from(subscriber_id.as_u8()),
        since_generation,
        wait_timeout_ns,
    }
    .encode();
    let response = request_control_frame_with_file_descriptors(
        socket_path,
        Opcode::NotifyStreamAdded,
        0,
        &payload,
        MAX_ATTACH_STREAMS_RESPONSE,
        MAX_ATTACH_STREAMS_PER_RESPONSE,
    )?;
    if response.frame.header.opcode != Opcode::NotifyStreamAdded {
        return Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_RBUS,
            "notify_stream_added::opcode",
        ));
    }
    let decoded = NotifyStreamAddedResponse::decode(&response.frame.payload)?;
    if decoded.streams.len() != response.file_descriptors.len() {
        return Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_RBUS,
            "notify_stream_added::fd_count",
        ));
    }

    let mut attached_streams = Vec::with_capacity(decoded.streams.len());
    for (metadata, file_descriptor) in decoded
        .streams
        .iter()
        .copied()
        .zip(response.file_descriptors)
    {
        attached_streams.push(AttachedStream {
            metadata: StreamSegmentMetadata {
                stream_id: StreamId::new(metadata.stream_id),
                publisher_id: PublisherId::new(metadata.publisher_id),
                topic_hash,
                segment_size: u64_to_usize(
                    metadata.segment_size,
                    "notify_stream_added::segment_size",
                )?,
                slot_size: u64_to_usize(metadata.slot_size, "notify_stream_added::slot_size")?,
                depth: u64_to_usize(metadata.depth, "notify_stream_added::depth")?,
            },
            stream_generation: metadata.stream_generation,
            file_descriptor,
        });
    }
    Ok(StreamNotification {
        latest_generation: decoded.latest_generation,
        streams: attached_streams,
    })
}

fn u64_to_usize(value: u64, operation: &'static str) -> Result<usize, Warning> {
    usize::try_from(value)
        .map_err(|_| Warning::new(WarningCode::ResourceLimit, COMPONENT_RBUS, operation))
}
