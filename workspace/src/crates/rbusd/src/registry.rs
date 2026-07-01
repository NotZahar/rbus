use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use rbus_core::constants::MAX_SUBSCRIBERS_PER_STREAM;
use rbus_core::ids::{PublisherId, StreamId, SubscriberId, TopicHash};
use rbus_core::ring::pin::reader_bit_mask;
use rbus_core::warning::{Warning, WarningCode};
use rbus_ipc::control::messages::{
    AttachStreamsResponse, NotifyStreamAddedResponse, RegisterPublisherRequest,
    RegisterPublisherResponse, RegisterSubscriberResponse, StreamAttachment, UnregisterResponse,
};
use rbus_ipc::stream_segment::create_stream_segment;
use rbus_ipc::topic_wait::{TopicWaitSegment, create_topic_wait_segment};

use crate::registry_types::{
    AttachStreamsResult, DaemonStream, NotifyStreamAddedResult, PublisherRecord,
    RegisteredPublisher, RegisteredSubscriber, RegistrySnapshot, SubscriberRecord, TopicEntry,
};

const COMPONENT_RBUSD: &str = "rbusd";
const FIRST_PUBLISHER_ID: u64 = 1;
const FIRST_STREAM_ID: u64 = 1;
const UNREGISTER_REMOVED_ENTRY_COUNT: u64 = 1;

/// In-memory registry of active topics, publishers and subscribers
#[derive(Default)]
pub struct DaemonRegistry {
    pub(crate) topics: BTreeMap<TopicHash, TopicEntry>,
    pub(crate) publishers: BTreeMap<u64, PublisherRecord>,
    pub(crate) subscribers: BTreeMap<u64, SubscriberRecord>,
    pub(crate) next_publisher_id: u64,
    pub(crate) next_stream_id: u64,
    pub(crate) active_subscribers_mask: u64,
    pub(crate) generation: u64,
}

impl DaemonRegistry {
    /// Creates an empty registry with ids starting at one
    pub fn new() -> Self {
        Self {
            topics: BTreeMap::new(),
            publishers: BTreeMap::new(),
            subscribers: BTreeMap::new(),
            next_publisher_id: FIRST_PUBLISHER_ID,
            next_stream_id: FIRST_STREAM_ID,
            active_subscribers_mask: 0,
            generation: 0,
        }
    }

    /// Registers a publisher and creates its stream segment
    pub fn register_publisher(
        &mut self,
        request: RegisterPublisherRequest,
        session_id: u64,
    ) -> Result<RegisteredPublisher, Warning> {
        let publisher_id = PublisherId::new(self.next_publisher_id);
        self.next_publisher_id = self.next_publisher_id.checked_add(1).ok_or_else(|| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_RBUSD,
                "DaemonRegistry::register_publisher::publisher_id",
            )
        })?;
        let stream_id = StreamId::new(self.next_stream_id);
        self.next_stream_id = self.next_stream_id.checked_add(1).ok_or_else(|| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_RBUSD,
                "DaemonRegistry::register_publisher::stream_id",
            )
        })?;
        self.generation = self.generation.checked_add(1).ok_or_else(|| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_RBUSD,
                "DaemonRegistry::register_publisher::generation",
            )
        })?;

        let topic_hash = TopicHash::new(request.topic_hash);
        let slot_size = usize::try_from(request.max_message_size).map_err(|_| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_RBUSD,
                "DaemonRegistry::register_publisher::slot_size",
            )
        })?;
        let depth = usize::try_from(request.depth).map_err(|_| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_RBUSD,
                "DaemonRegistry::register_publisher::depth",
            )
        })?;
        let created_segment =
            create_stream_segment(publisher_id, stream_id, topic_hash, slot_size, depth)?;
        let stream_generation = self.generation;
        let subscriber_count = self.topic_entry_mut(topic_hash)?.subscriber_count;
        let response = RegisterPublisherResponse {
            publisher_id: publisher_id.as_u64(),
            stream_id: stream_id.as_u64(),
            stream_generation,
            segment_size: created_segment.metadata.segment_size as u64,
            slot_size: created_segment.metadata.slot_size as u64,
            depth: created_segment.metadata.depth as u64,
            subscriber_count,
        };
        let (file_descriptor, topic_wait_file_descriptor) = {
            let topic_entry = self.topic_entry_mut(topic_hash)?;
            topic_entry.streams.push(DaemonStream {
                metadata: StreamAttachment {
                    publisher_id: response.publisher_id,
                    stream_id: response.stream_id,
                    stream_generation: response.stream_generation,
                    segment_size: response.segment_size,
                    slot_size: response.slot_size,
                    depth: response.depth,
                },
                topic_hash,
                file_descriptor: created_segment.file_descriptor,
            });
            TopicWaitSegment::map(&topic_entry.topic_wait_file_descriptor)?.wake_stream_added()?;
            let stream_file_descriptor = topic_entry
                .streams
                .last()
                .ok_or_else(|| {
                    Warning::new(
                        WarningCode::InternalInvariantWarning,
                        COMPONENT_RBUSD,
                        "DaemonRegistry::register_publisher::last_stream",
                    )
                })?
                .file_descriptor
                .as_raw_file_descriptor();
            (
                stream_file_descriptor,
                topic_entry
                    .topic_wait_file_descriptor
                    .as_raw_file_descriptor(),
            )
        };
        self.publishers.insert(
            publisher_id.as_u64(),
            PublisherRecord {
                topic_hash,
                session_id,
            },
        );
        Ok(RegisteredPublisher {
            response,
            file_descriptor,
            topic_wait_file_descriptor,
        })
    }

    /// Registers a subscriber and returns the topic wait descriptor
    pub fn register_subscriber(
        &mut self,
        topic_hash: TopicHash,
        session_id: u64,
    ) -> Result<RegisteredSubscriber, Warning> {
        let subscriber_id = self.allocate_subscriber_id()?;
        self.generation = self.generation.checked_add(1).ok_or_else(|| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_RBUSD,
                "DaemonRegistry::register_subscriber::generation",
            )
        })?;
        self.subscribers.insert(
            subscriber_id,
            SubscriberRecord {
                topic_hash,
                session_id,
            },
        );
        let subscribed_generation = self.generation;
        let topic_wait_file_descriptor = {
            let topic_entry = self.topic_entry_mut(topic_hash)?;
            topic_entry.subscriber_count =
                topic_entry.subscriber_count.checked_add(1).ok_or_else(|| {
                    Warning::new(
                        WarningCode::ResourceLimit,
                        COMPONENT_RBUSD,
                        "DaemonRegistry::register_subscriber::topic_count",
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

    /// Returns all streams currently attached to a topic
    pub fn attach_streams(&self, topic_hash: TopicHash) -> AttachStreamsResult {
        self.collect_streams(topic_hash, 0)
    }

    /// Returns streams created after `since_generation`
    pub fn notify_stream_added(
        &self,
        topic_hash: TopicHash,
        subscriber_id: SubscriberId,
        since_generation: u64,
    ) -> Result<NotifyStreamAddedResult, Warning> {
        let subscriber_record = self
            .subscribers
            .get(&u64::from(subscriber_id.as_u8()))
            .ok_or_else(|| {
                Warning::new(
                    WarningCode::Disconnected,
                    COMPONENT_RBUSD,
                    "DaemonRegistry::notify_stream_added::subscriber",
                )
            })?;
        if subscriber_record.topic_hash != topic_hash {
            return Err(Warning::new(
                WarningCode::ProtocolMismatch,
                COMPONENT_RBUSD,
                "DaemonRegistry::notify_stream_added::topic",
            ));
        }

        let attached = self.collect_streams(topic_hash, since_generation);
        Ok(NotifyStreamAddedResult {
            response: NotifyStreamAddedResponse {
                latest_generation: self.generation,
                streams: attached.response.streams,
            },
            file_descriptors: attached.file_descriptors,
        })
    }

    /// Removes a publisher and marks its stream closed
    pub fn unregister_publisher(
        &mut self,
        publisher_id: PublisherId,
    ) -> Result<UnregisterResponse, Warning> {
        let publisher_record = self
            .publishers
            .remove(&publisher_id.as_u64())
            .ok_or_else(|| {
                Warning::new(
                    WarningCode::Disconnected,
                    COMPONENT_RBUSD,
                    "DaemonRegistry::unregister_publisher::publisher",
                )
            })?;
        let topic_hash = publisher_record.topic_hash;
        let topic_entry = self.topics.get_mut(&topic_hash).ok_or_else(|| {
            Warning::new(
                WarningCode::InternalInvariantWarning,
                COMPONENT_RBUSD,
                "DaemonRegistry::unregister_publisher::topic",
            )
        })?;
        let stream_index = topic_entry
            .streams
            .iter()
            .position(|stream| stream.metadata.publisher_id == publisher_id.as_u64())
            .ok_or_else(|| {
                Warning::new(
                    WarningCode::InternalInvariantWarning,
                    COMPONENT_RBUSD,
                    "DaemonRegistry::unregister_publisher::stream",
                )
            })?;
        topic_entry.streams[stream_index].mark_closed()?;
        topic_entry.streams.remove(stream_index);
        let registry_generation = self.increment_generation("unregister_publisher")?;
        self.prune_empty_topic(topic_hash);
        Ok(UnregisterResponse {
            removed_count: UNREGISTER_REMOVED_ENTRY_COUNT,
            registry_generation,
        })
    }

    /// Removes a subscriber and clears its reader pins
    pub fn unregister_subscriber(
        &mut self,
        subscriber_id: SubscriberId,
    ) -> Result<UnregisterResponse, Warning> {
        let subscriber_id_u64 = u64::from(subscriber_id.as_u8());
        let subscriber_record = self.subscribers.remove(&subscriber_id_u64).ok_or_else(|| {
            Warning::new(
                WarningCode::Disconnected,
                COMPONENT_RBUSD,
                "DaemonRegistry::unregister_subscriber::subscriber",
            )
        })?;
        let topic_hash = subscriber_record.topic_hash;
        self.clear_topic_waiter_bit(topic_hash, subscriber_id)?;
        let _cleared_reader_bits = self.clear_subscriber_reader_bits(subscriber_id)?;
        self.active_subscribers_mask &= !(1_u64 << u32::from(subscriber_id.as_u8()));
        let topic_entry = self.topics.get_mut(&topic_hash).ok_or_else(|| {
            Warning::new(
                WarningCode::InternalInvariantWarning,
                COMPONENT_RBUSD,
                "DaemonRegistry::unregister_subscriber::topic",
            )
        })?;
        topic_entry.subscriber_count =
            topic_entry.subscriber_count.checked_sub(1).ok_or_else(|| {
                Warning::new(
                    WarningCode::InternalInvariantWarning,
                    COMPONENT_RBUSD,
                    "DaemonRegistry::unregister_subscriber::topic_count",
                )
            })?;
        let registry_generation = self.increment_generation("unregister_subscriber")?;
        self.prune_empty_topic(topic_hash);
        Ok(UnregisterResponse {
            removed_count: UNREGISTER_REMOVED_ENTRY_COUNT,
            registry_generation,
        })
    }

    /// Cleans all publishers and subscribers owned by a control session
    pub fn cleanup_session(&mut self, session_id: u64) -> Result<u64, Warning> {
        let publisher_ids: Vec<PublisherId> = self
            .publishers
            .iter()
            .filter_map(|(publisher_id, record)| {
                (record.session_id == session_id).then_some(PublisherId::new(*publisher_id))
            })
            .collect();
        let subscriber_ids: Vec<SubscriberId> = self
            .subscribers
            .iter()
            .filter_map(|(subscriber_id, record)| {
                (record.session_id == session_id).then_some(SubscriberId::new(*subscriber_id as u8))
            })
            .collect();
        let mut removed_count = 0_u64;
        for subscriber_id in subscriber_ids {
            let _response = self.unregister_subscriber(subscriber_id)?;
            removed_count = removed_count.checked_add(1).ok_or_else(|| {
                Warning::new(
                    WarningCode::ResourceLimit,
                    COMPONENT_RBUSD,
                    "DaemonRegistry::cleanup_session::subscriber_count",
                )
            })?;
        }
        for publisher_id in publisher_ids {
            let _response = self.unregister_publisher(publisher_id)?;
            removed_count = removed_count.checked_add(1).ok_or_else(|| {
                Warning::new(
                    WarningCode::ResourceLimit,
                    COMPONENT_RBUSD,
                    "DaemonRegistry::cleanup_session::publisher_count",
                )
            })?;
        }
        Ok(removed_count)
    }

    fn collect_streams(&self, topic_hash: TopicHash, since_generation: u64) -> AttachStreamsResult {
        let mut streams = Vec::new();
        let mut file_descriptors = Vec::new();
        if let Some(topic_entry) = self.topics.get(&topic_hash) {
            streams.reserve(topic_entry.streams.len());
            file_descriptors.reserve(topic_entry.streams.len());
            for stream in topic_entry
                .streams
                .iter()
                .filter(|stream| stream.metadata.stream_generation > since_generation)
            {
                streams.push(stream.metadata);
                file_descriptors.push(stream.file_descriptor.as_raw_file_descriptor());
            }
        }
        AttachStreamsResult {
            response: AttachStreamsResponse { streams },
            file_descriptors,
        }
    }

    fn clear_subscriber_reader_bits(&self, subscriber_id: SubscriberId) -> Result<u64, Warning> {
        let mut cleared_count = 0_u64;
        for topic_entry in self.topics.values() {
            for stream in &topic_entry.streams {
                cleared_count = cleared_count
                    .checked_add(stream.clear_reader_bit(subscriber_id)?)
                    .ok_or_else(|| {
                        Warning::new(
                            WarningCode::ResourceLimit,
                            COMPONENT_RBUSD,
                            "DaemonRegistry::clear_subscriber_reader_bits",
                        )
                    })?;
            }
        }
        Ok(cleared_count)
    }

    fn clear_topic_waiter_bit(
        &self,
        topic_hash: TopicHash,
        subscriber_id: SubscriberId,
    ) -> Result<(), Warning> {
        if let Some(topic_entry) = self.topics.get(&topic_hash) {
            TopicWaitSegment::map(&topic_entry.topic_wait_file_descriptor)?
                .remove_waiter(reader_bit_mask(subscriber_id)?);
        }
        Ok(())
    }

    fn allocate_subscriber_id(&mut self) -> Result<u64, Warning> {
        let subscriber_id = (!self.active_subscribers_mask).trailing_zeros();
        if subscriber_id >= u32::from(MAX_SUBSCRIBERS_PER_STREAM) {
            return Err(Warning::new(
                WarningCode::SubscriberLimitReached,
                COMPONENT_RBUSD,
                "DaemonRegistry::allocate_subscriber_id",
            ));
        }
        self.active_subscribers_mask |= 1_u64 << subscriber_id;
        Ok(u64::from(subscriber_id))
    }

    fn increment_generation(&mut self, operation: &'static str) -> Result<u64, Warning> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| Warning::new(WarningCode::ResourceLimit, COMPONENT_RBUSD, operation))?;
        Ok(self.generation)
    }

    fn prune_empty_topic(&mut self, topic_hash: TopicHash) {
        let empty = self.topics.get(&topic_hash).is_some_and(|topic_entry| {
            topic_entry.streams.is_empty() && topic_entry.subscriber_count == 0
        });
        if empty {
            self.topics.remove(&topic_hash);
        }
    }

    fn topic_entry_mut(&mut self, topic_hash: TopicHash) -> Result<&mut TopicEntry, Warning> {
        match self.topics.entry(topic_hash) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                let topic_wait_file_descriptor = create_topic_wait_segment(topic_hash.as_u64())?;
                Ok(entry.insert(TopicEntry {
                    streams: Vec::new(),
                    subscriber_count: 0,
                    topic_wait_file_descriptor,
                }))
            }
        }
    }

    /// Returns the number of active publisher streams
    pub fn publisher_count(&self) -> u64 {
        self.topics
            .values()
            .map(|topic_entry| topic_entry.streams.len() as u64)
            .sum()
    }

    /// Returns the number of active subscribers
    pub fn subscriber_count(&self) -> u64 {
        self.topics
            .values()
            .map(|topic_entry| topic_entry.subscriber_count)
            .sum()
    }

    /// Returns the number of non-empty topics
    pub fn topic_count(&self) -> u64 {
        self.topics.len() as u64
    }

    /// Returns the number of active streams
    pub fn stream_count(&self) -> u64 {
        self.publisher_count()
    }

    /// Captures aggregate registry counters
    pub fn snapshot(&self) -> RegistrySnapshot {
        RegistrySnapshot {
            topic_count: self.topic_count(),
            publisher_count: self.publisher_count(),
            subscriber_count: self.subscriber_count(),
            stream_count: self.stream_count(),
            generation: self.generation,
        }
    }
}
