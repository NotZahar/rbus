use std::os::fd::RawFd;

use rbus_core::ids::{PublisherId, StreamId, SubscriberId, TopicHash};
use rbus_core::warning::Warning;
use rbus_ipc::control::messages::{
    AttachStreamsResponse, NotifyStreamAddedResponse, RegisterPublisherResponse,
    RegisterSubscriberResponse, StreamAttachment,
};
use rbus_ipc::fd::OwnedFileDescriptor;
use rbus_ipc::stream_segment::{
    StreamSegmentMetadata, clear_subscriber_reader_bits, mark_stream_closed,
};

/// Daemon runtime metadata
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RegistrySnapshot {
    /// Topic runtime metadata
    pub topic_count: u64,
    /// Publisher runtime metadata
    pub publisher_count: u64,
    /// Subscriber runtime metadata
    pub subscriber_count: u64,
    /// Stream runtime metadata
    pub stream_count: u64,
    /// Registry generation
    pub generation: u64,
}

/// Internal documentation
pub struct RegisteredPublisher {
    /// Encoded response payload fields
    pub response: RegisterPublisherResponse,
    /// Daemon runtime metadata
    pub file_descriptor: RawFd,
    /// Topic runtime metadata
    pub topic_wait_file_descriptor: RawFd,
}

/// Internal documentation
pub struct RegisteredSubscriber {
    /// Encoded response payload fields
    pub response: RegisterSubscriberResponse,
    /// Topic runtime metadata
    pub topic_wait_file_descriptor: RawFd,
}

/// Internal documentation
pub struct AttachStreamsResult {
    /// Response payload
    pub response: AttachStreamsResponse,
    /// Stream runtime metadata
    pub file_descriptors: Vec<RawFd>,
}

/// Internal documentation
pub struct NotifyStreamAddedResult {
    /// Response payload
    pub response: NotifyStreamAddedResponse,
    /// Stream runtime metadata
    pub file_descriptors: Vec<RawFd>,
}

pub(crate) struct TopicEntry {
    pub(crate) streams: Vec<DaemonStream>,
    pub(crate) subscriber_count: u64,
    pub(crate) topic_wait_file_descriptor: OwnedFileDescriptor,
}

pub(crate) struct DaemonStream {
    pub(crate) metadata: StreamAttachment,
    pub(crate) topic_hash: TopicHash,
    pub(crate) file_descriptor: OwnedFileDescriptor,
}

impl DaemonStream {
    pub(crate) fn mark_closed(&self) -> Result<(), Warning> {
        mark_stream_closed(&self.file_descriptor, self.segment_metadata())
    }

    pub(crate) fn clear_reader_bit(&self, subscriber_id: SubscriberId) -> Result<u64, Warning> {
        clear_subscriber_reader_bits(
            &self.file_descriptor,
            self.segment_metadata(),
            subscriber_id,
        )
    }

    pub(crate) fn segment_metadata(&self) -> StreamSegmentMetadata {
        StreamSegmentMetadata {
            stream_id: StreamId::new(self.metadata.stream_id),
            publisher_id: PublisherId::new(self.metadata.publisher_id),
            topic_hash: self.topic_hash,
            segment_size: self.metadata.segment_size as usize,
            slot_size: self.metadata.slot_size as usize,
            depth: self.metadata.depth as usize,
        }
    }
}

pub(crate) struct PublisherRecord {
    pub(crate) topic_hash: TopicHash,
    pub(crate) session_id: u64,
}

pub(crate) struct SubscriberRecord {
    pub(crate) topic_hash: TopicHash,
    pub(crate) session_id: u64,
}
