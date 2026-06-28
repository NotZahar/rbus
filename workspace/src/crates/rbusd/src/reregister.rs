use std::collections::btree_map::Entry;

use rbus_core::constants::MAX_SUBSCRIBERS_PER_STREAM;
use rbus_core::ids::{PublisherId, StreamId, TopicHash};
use rbus_core::warning::{Warning, WarningCode};
use rbus_ipc::control::FrameHeader;
use rbus_ipc::control::io::ControlFrame;
use rbus_ipc::control::messages::RegisterSubscriberResponse;
use rbus_ipc::control::opcode::Opcode;
use rbus_ipc::control::reregister::{
    ReregisterPublisherRequest, ReregisterPublisherResponse, ReregisterSubscriberRequest,
};
use rbus_ipc::fd::OwnedFileDescriptor;
use rbus_ipc::stream_segment::{StreamSegmentMetadata, map_stream_segment};
use rbus_ipc::topic_wait::TopicWaitSegment;

use crate::daemon::DaemonResponse;
use crate::registry::DaemonRegistry;
use crate::registry_types::{
    DaemonStream, PublisherRecord, RegisteredSubscriber, SubscriberRecord, TopicEntry,
};

const COMPONENT_RBUSD: &str = "rbusd";
const REREGISTER_PUBLISHER_FD_COUNT: usize = 2;
const REREGISTER_SUBSCRIBER_FD_COUNT: usize = 1;
const STREAM_FD_INDEX: usize = 0;
const TOPIC_WAIT_FD_INDEX: usize = 1;

pub(crate) fn reregister_publisher_response(
    registry: &mut DaemonRegistry,
    session_id: u64,
    payload: &[u8],
    file_descriptors: Vec<OwnedFileDescriptor>,
) -> Result<DaemonResponse, Warning> {
    if file_descriptors.len() != REREGISTER_PUBLISHER_FD_COUNT {
        return Err(protocol_warning("reregister_publisher_response::fd_count"));
    }
    let request = ReregisterPublisherRequest::decode(payload)?;
    let mut file_descriptors = file_descriptors;
    let topic_wait_file_descriptor = file_descriptors.remove(TOPIC_WAIT_FD_INDEX);
    let stream_file_descriptor = file_descriptors.remove(STREAM_FD_INDEX);
    let response = reregister_publisher(
        registry,
        request,
        stream_file_descriptor,
        topic_wait_file_descriptor,
        session_id,
    )?;
    let response_payload = response.encode();
    Ok(DaemonResponse {
        frame: ControlFrame {
            header: FrameHeader::new(
                Opcode::ReregisterPublisher,
                0,
                control_payload_length(response_payload.len(), "reregister_publisher_response")?,
            ),
            payload: response_payload.to_vec(),
        },
        file_descriptors: Vec::new(),
    })
}

pub(crate) fn reregister_subscriber_response(
    registry: &mut DaemonRegistry,
    session_id: u64,
    payload: &[u8],
    file_descriptors: Vec<OwnedFileDescriptor>,
) -> Result<DaemonResponse, Warning> {
    if file_descriptors.len() != REREGISTER_SUBSCRIBER_FD_COUNT {
        return Err(protocol_warning("reregister_subscriber_response::fd_count"));
    }
    let request = ReregisterSubscriberRequest::decode(payload)?;
    let mut file_descriptors = file_descriptors;
    let registered = reregister_subscriber(
        registry,
        TopicHash::new(request.topic_hash),
        file_descriptors.remove(STREAM_FD_INDEX),
        session_id,
    )?;
    let response_payload = registered.response.encode();
    Ok(DaemonResponse {
        frame: ControlFrame {
            header: FrameHeader::new(
                Opcode::ReregisterSubscriber,
                0,
                control_payload_length(response_payload.len(), "reregister_subscriber_response")?,
            ),
            payload: response_payload.to_vec(),
        },
        file_descriptors: vec![registered.topic_wait_file_descriptor],
    })
}

fn reregister_publisher(
    registry: &mut DaemonRegistry,
    request: ReregisterPublisherRequest,
    stream_file_descriptor: OwnedFileDescriptor,
    topic_wait_file_descriptor: OwnedFileDescriptor,
    session_id: u64,
) -> Result<ReregisterPublisherResponse, Warning> {
    let metadata = stream_metadata(request)?;
    map_stream_segment(&stream_file_descriptor, metadata, true)?;
    reject_duplicate_publisher(registry, metadata.publisher_id)?;
    reject_duplicate_stream(registry, metadata.stream_id)?;
    advance_next_id(
        &mut registry.next_publisher_id,
        metadata.publisher_id.as_u64(),
    )?;
    advance_next_id(&mut registry.next_stream_id, metadata.stream_id.as_u64())?;
    let stream_generation = increment_generation(registry, "reregister_publisher::generation")?;
    let subscriber_count = {
        let topic_entry =
            topic_entry_from_fd(registry, metadata.topic_hash, topic_wait_file_descriptor);
        let subscriber_count = topic_entry.subscriber_count;
        topic_entry.streams.push(DaemonStream {
            metadata: rbus_ipc::control::messages::StreamAttachment {
                publisher_id: metadata.publisher_id.as_u64(),
                stream_id: metadata.stream_id.as_u64(),
                stream_generation,
                segment_size: metadata.segment_size as u64,
                slot_size: metadata.slot_size as u64,
                depth: metadata.depth as u64,
            },
            topic_hash: metadata.topic_hash,
            file_descriptor: stream_file_descriptor,
        });
        TopicWaitSegment::map(&topic_entry.topic_wait_file_descriptor)?.wake_stream_added()?;
        subscriber_count
    };
    registry.publishers.insert(
        metadata.publisher_id.as_u64(),
        PublisherRecord {
            topic_hash: metadata.topic_hash,
            session_id,
        },
    );
    Ok(ReregisterPublisherResponse {
        stream_generation,
        subscriber_count,
    })
}

fn reregister_subscriber(
    registry: &mut DaemonRegistry,
    topic_hash: TopicHash,
    topic_wait_file_descriptor: OwnedFileDescriptor,
    session_id: u64,
) -> Result<RegisteredSubscriber, Warning> {
    let subscriber_id = allocate_subscriber_id(registry)?;
    let subscribed_generation = increment_generation(registry, "reregister_subscriber")?;
    registry.subscribers.insert(
        subscriber_id,
        SubscriberRecord {
            topic_hash,
            session_id,
        },
    );
    let topic_wait_file_descriptor = {
        let topic_entry = topic_entry_from_fd(registry, topic_hash, topic_wait_file_descriptor);
        topic_entry.subscriber_count =
            topic_entry.subscriber_count.checked_add(1).ok_or_else(|| {
                Warning::new(
                    WarningCode::ResourceLimit,
                    COMPONENT_RBUSD,
                    "reregister_subscriber::topic_count",
                )
            })?;
        topic_entry
            .topic_wait_file_descriptor
            .as_raw_file_descriptor()
    };
    Ok(RegisteredSubscriber {
        response: RegisterSubscriberResponse {
            subscriber_id,
            subscribed_generation,
        },
        topic_wait_file_descriptor,
    })
}

fn stream_metadata(request: ReregisterPublisherRequest) -> Result<StreamSegmentMetadata, Warning> {
    Ok(StreamSegmentMetadata {
        stream_id: StreamId::new(request.stream_id),
        publisher_id: PublisherId::new(request.publisher_id),
        topic_hash: TopicHash::new(request.topic_hash),
        segment_size: usize::try_from(request.segment_size).map_err(|_| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_RBUSD,
                "reregister_publisher::segment_size",
            )
        })?,
        slot_size: usize::try_from(request.slot_size).map_err(|_| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_RBUSD,
                "reregister_publisher::slot_size",
            )
        })?,
        depth: usize::try_from(request.depth).map_err(|_| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_RBUSD,
                "reregister_publisher::depth",
            )
        })?,
    })
}

fn topic_entry_from_fd(
    registry: &mut DaemonRegistry,
    topic_hash: TopicHash,
    topic_wait_file_descriptor: OwnedFileDescriptor,
) -> &mut TopicEntry {
    match registry.topics.entry(topic_hash) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => entry.insert(TopicEntry {
            streams: Vec::new(),
            subscriber_count: 0,
            topic_wait_file_descriptor,
        }),
    }
}

fn reject_duplicate_publisher(
    registry: &DaemonRegistry,
    publisher_id: PublisherId,
) -> Result<(), Warning> {
    if registry.publishers.contains_key(&publisher_id.as_u64()) {
        return Err(protocol_warning("reregister_publisher::publisher"));
    }
    Ok(())
}

fn reject_duplicate_stream(registry: &DaemonRegistry, stream_id: StreamId) -> Result<(), Warning> {
    if registry.topics.values().any(|topic_entry| {
        topic_entry
            .streams
            .iter()
            .any(|stream| stream.metadata.stream_id == stream_id.as_u64())
    }) {
        return Err(protocol_warning("reregister_publisher::stream"));
    }
    Ok(())
}

fn allocate_subscriber_id(registry: &mut DaemonRegistry) -> Result<u64, Warning> {
    let subscriber_id = (!registry.active_subscribers_mask).trailing_zeros();
    if subscriber_id >= u32::from(MAX_SUBSCRIBERS_PER_STREAM) {
        return Err(Warning::new(
            WarningCode::SubscriberLimitReached,
            COMPONENT_RBUSD,
            "reregister_subscriber::subscriber_id",
        ));
    }
    registry.active_subscribers_mask |= 1_u64 << subscriber_id;
    Ok(u64::from(subscriber_id))
}

fn advance_next_id(next_id: &mut u64, used_id: u64) -> Result<(), Warning> {
    let next_after_used = used_id.checked_add(1).ok_or_else(|| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_RBUSD,
            "reregister::advance_next_id",
        )
    })?;
    *next_id = (*next_id).max(next_after_used);
    Ok(())
}

fn increment_generation(
    registry: &mut DaemonRegistry,
    operation: &'static str,
) -> Result<u64, Warning> {
    registry.generation = registry
        .generation
        .checked_add(1)
        .ok_or_else(|| Warning::new(WarningCode::ResourceLimit, COMPONENT_RBUSD, operation))?;
    Ok(registry.generation)
}

fn control_payload_length(length: usize, operation: &'static str) -> Result<u32, Warning> {
    u32::try_from(length)
        .map_err(|_| Warning::new(WarningCode::ResourceLimit, COMPONENT_RBUSD, operation))
}

fn protocol_warning(operation: &'static str) -> Warning {
    Warning::new(WarningCode::ProtocolMismatch, COMPONENT_RBUSD, operation)
}
