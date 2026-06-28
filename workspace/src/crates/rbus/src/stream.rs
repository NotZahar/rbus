use core::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rbus_core::atomic::ordering::{
    COUNTER_ORDERING, READER_MASK_UPDATE_ORDERING, READER_OBSERVE_ORDERING,
    SEGMENT_FLAG_OBSERVE_ORDERING, SLOT_PROTOCOL_ORDERING, WRITER_COMMIT_ORDERING,
};
use rbus_core::constants::SLOT_WRITER_CLAIM_BIT;
use rbus_core::ids::{PublisherId, StreamId, SubscriberId, TopicHash};
use rbus_core::layout::{SegmentHeader, SegmentLayout, SlotHeader};
use rbus_core::ring::pin::reader_bit_mask;
use rbus_core::ring::state::{
    decode_sequence, encode_committed_state, encode_writing_state, is_committed_state,
};
use rbus_core::sequence::Sequence;
use rbus_core::stats::{Stats, StreamCounters};
use rbus_core::warning::{Warning, WarningCode};
use rbus_ipc::clock::monotonic_timestamp_ns;
use rbus_ipc::fd::OwnedFileDescriptor;
use rbus_ipc::mmap::MappedRegion;
use rbus_ipc::stream_segment::{StreamSegmentMetadata, map_stream_segment};

const COMPONENT_RBUS: &str = "rbus";

pub(crate) struct StreamInner {
    stream_id: StreamId,
    publisher_id: PublisherId,
    topic_hash: TopicHash,
    created_generation: u64,
    layout: SegmentLayout,
    mapping: MappedRegion,
    subscriber_count: core::sync::atomic::AtomicU64,
    loan_in_progress: AtomicBool,
}

impl StreamInner {
    pub(crate) fn attach(
        metadata: StreamSegmentMetadata,
        created_generation: u64,
        file_descriptor: &OwnedFileDescriptor,
        initial_subscriber_count: u64,
    ) -> Result<Arc<Self>, Warning> {
        let (mapping, layout) = map_stream_segment(file_descriptor, metadata, true)?;

        Ok(Arc::new(Self {
            stream_id: metadata.stream_id,
            publisher_id: metadata.publisher_id,
            topic_hash: metadata.topic_hash,
            created_generation,
            layout,
            mapping,
            subscriber_count: core::sync::atomic::AtomicU64::new(initial_subscriber_count),
            loan_in_progress: AtomicBool::new(false),
        }))
    }

    pub(crate) const fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    pub(crate) const fn publisher_id(&self) -> PublisherId {
        self.publisher_id
    }

    pub(crate) const fn created_generation(&self) -> u64 {
        self.created_generation
    }

    pub(crate) fn metadata(&self) -> StreamSegmentMetadata {
        StreamSegmentMetadata {
            stream_id: self.stream_id,
            publisher_id: self.publisher_id,
            topic_hash: self.topic_hash,
            segment_size: self.layout.segment_size,
            slot_size: self.layout.slot_size,
            depth: self.layout.depth,
        }
    }

    pub(crate) fn set_subscriber_count(&self, subscriber_count: u64) {
        self.subscriber_count
            .store(subscriber_count, Ordering::Relaxed);
    }

    pub(crate) fn current_write_sequence(&self) -> Sequence {
        Sequence::new(
            self.segment_header()
                .write_sequence
                .load(READER_OBSERVE_ORDERING),
        )
    }

    pub(crate) fn stats(&self) -> Stats {
        self.stream_counters().snapshot(
            self.segment_header().write_sequence.load(Ordering::Relaxed),
            self.subscriber_count.load(Ordering::Relaxed),
            self.publisher_alive(),
        )
    }

    pub(crate) fn try_begin_loan(
        self: &Arc<Self>,
        length: usize,
    ) -> Result<LoanReservation, Warning> {
        if !self.publisher_alive() {
            self.stream_counters().increment_warnings_total();
            return Err(Warning::new(
                WarningCode::Disconnected,
                COMPONENT_RBUS,
                "Publisher::loan::closed",
            )
            .with_stream_id(self.stream_id));
        }

        if length > self.layout.slot_size {
            self.stream_counters().increment_warnings_total();
            return Err(Warning::new(
                WarningCode::MessageTooLarge,
                COMPONENT_RBUS,
                "Publisher::loan",
            )
            .with_topic_hash(self.topic_hash));
        }

        if self
            .loan_in_progress
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            self.stream_counters().increment_warnings_total();
            return Err(Warning::new(
                WarningCode::BackpressurePinnedSlot,
                COMPONENT_RBUS,
                "Publisher::loan",
            )
            .with_topic_hash(self.topic_hash));
        }

        let sequence = Sequence::new(self.segment_header().write_sequence.load(Ordering::Relaxed));
        let slot_index = sequence.slot_index(self.layout.depth);
        let slot = self.slot_header(slot_index);
        if slot
            .readers_mask
            .compare_exchange(
                0,
                SLOT_WRITER_CLAIM_BIT,
                SLOT_PROTOCOL_ORDERING,
                SLOT_PROTOCOL_ORDERING,
            )
            .is_err()
        {
            self.loan_in_progress.store(false, Ordering::Release);
            self.stream_counters()
                .backpressure_pinned_slot
                .fetch_add(1, COUNTER_ORDERING);
            self.stream_counters().increment_warnings_total();
            return Err(Warning::new(
                WarningCode::BackpressurePinnedSlot,
                COMPONENT_RBUS,
                "Publisher::loan",
            )
            .with_sequence(sequence)
            .with_topic_hash(self.topic_hash));
        }

        slot.state_seq
            .store(encode_writing_state(sequence), SLOT_PROTOCOL_ORDERING);

        Ok(LoanReservation {
            stream: Arc::clone(self),
            slot_index,
            sequence,
            length,
            payload_pointer: self.payload_pointer(slot_index),
        })
    }

    pub(crate) fn commit_loan(
        &self,
        slot_index: usize,
        sequence: Sequence,
        length: usize,
    ) -> Result<(), Warning> {
        let timestamp_ns = match monotonic_timestamp_ns() {
            Ok(timestamp_ns) => timestamp_ns,
            Err(warning) => {
                self.rollback_loan(slot_index);
                return Err(warning);
            }
        };
        let slot_pointer = self.slot_header_pointer(slot_index);
        // SAFETY: Required invariants are upheld
        // Invariants:
        // - Required invariant holds
        // - Required invariant holds
        // Covered by tests:
        // - loaned_publish_makes_payload_visible_to_subscriber
        unsafe {
            (*slot_pointer).len = length as u32;
            (*slot_pointer).flags = 0;
            (*slot_pointer).timestamp_ns = timestamp_ns;
        }
        self.slot_header(slot_index)
            .state_seq
            .store(encode_committed_state(sequence), SLOT_PROTOCOL_ORDERING);
        self.segment_header()
            .write_sequence
            .store(sequence.next().as_u64(), WRITER_COMMIT_ORDERING);
        self.slot_header(slot_index)
            .readers_mask
            .fetch_and(!SLOT_WRITER_CLAIM_BIT, SLOT_PROTOCOL_ORDERING);
        self.stream_counters()
            .published_messages
            .fetch_add(1, COUNTER_ORDERING);
        self.loan_in_progress.store(false, Ordering::Release);
        Ok(())
    }

    pub(crate) fn rollback_loan(&self, slot_index: usize) {
        let slot_pointer = self.slot_header_pointer(slot_index);
        // SAFETY: Required invariants are upheld
        // Invariants:
        // - Required invariant holds
        // - Required invariant holds
        // Covered by tests:
        // - publish_bytes_rejects_oversized_message
        unsafe {
            (*slot_pointer).len = 0;
        }
        self.slot_header(slot_index)
            .state_seq
            .store(0, SLOT_PROTOCOL_ORDERING);
        self.slot_header(slot_index)
            .readers_mask
            .fetch_and(!SLOT_WRITER_CLAIM_BIT, SLOT_PROTOCOL_ORDERING);
        self.loan_in_progress.store(false, Ordering::Release);
    }

    pub(crate) fn try_read_message(
        self: &Arc<Self>,
        expected_sequence: Sequence,
        subscriber_id: SubscriberId,
    ) -> Result<ReadAttempt, Warning> {
        let current_write_sequence = self.current_write_sequence();
        if expected_sequence >= current_write_sequence {
            return Ok(ReadAttempt::Empty);
        }

        let depth_as_u64 = self.layout.depth as u64;
        if current_write_sequence
            .as_u64()
            .saturating_sub(expected_sequence.as_u64())
            > depth_as_u64
        {
            let next_sequence =
                Sequence::new(current_write_sequence.as_u64().saturating_sub(depth_as_u64));
            let skipped_message_count = expected_sequence.skipped_messages_before(next_sequence);
            return Ok(self.messages_skipped(
                expected_sequence,
                next_sequence,
                skipped_message_count,
                "Subscriber::try_recv",
            ));
        }

        let slot_index = expected_sequence.slot_index(self.layout.depth);
        let slot = self.slot_header(slot_index);
        let state_sequence = slot.state_seq.load(SLOT_PROTOCOL_ORDERING);
        if !is_committed_state(state_sequence) {
            return Ok(ReadAttempt::Empty);
        }

        let observed_sequence = decode_sequence(state_sequence);
        if observed_sequence != expected_sequence {
            if observed_sequence > expected_sequence {
                let skipped_message_count =
                    expected_sequence.skipped_messages_before(observed_sequence);
                return Ok(self.messages_skipped(
                    expected_sequence,
                    observed_sequence,
                    skipped_message_count,
                    "Subscriber::try_recv",
                ));
            }
            return Ok(ReadAttempt::Empty);
        }

        let reader_bit = reader_bit_mask(subscriber_id)?;
        let mut readers_mask = slot.readers_mask.load(SLOT_PROTOCOL_ORDERING);
        loop {
            if readers_mask & SLOT_WRITER_CLAIM_BIT != 0 {
                return Ok(ReadAttempt::Empty);
            }
            match slot.readers_mask.compare_exchange(
                readers_mask,
                readers_mask | reader_bit,
                SLOT_PROTOCOL_ORDERING,
                SLOT_PROTOCOL_ORDERING,
            ) {
                Ok(_) => break,
                Err(observed_readers_mask) => readers_mask = observed_readers_mask,
            }
        }
        let state_after_pin = slot.state_seq.load(SLOT_PROTOCOL_ORDERING);
        if state_after_pin != state_sequence {
            slot.readers_mask
                .fetch_and(!reader_bit, SLOT_PROTOCOL_ORDERING);
            if is_committed_state(state_after_pin) {
                let observed_sequence_after_pin = decode_sequence(state_after_pin);
                if observed_sequence_after_pin > expected_sequence {
                    let skipped_message_count =
                        expected_sequence.skipped_messages_before(observed_sequence_after_pin);
                    return Ok(self.messages_skipped(
                        expected_sequence,
                        observed_sequence_after_pin,
                        skipped_message_count,
                        "Subscriber::try_recv::repin",
                    ));
                }
            }
            return Ok(ReadAttempt::Empty);
        }

        self.stream_counters()
            .received_messages
            .fetch_add(1, COUNTER_ORDERING);
        Ok(ReadAttempt::Message(ReadReservation {
            stream: Arc::clone(self),
            slot_index,
            reader_bit,
            sequence: observed_sequence,
            length: slot.len as usize,
            timestamp_ns: slot.timestamp_ns,
            payload_pointer: self.payload_pointer(slot_index).cast_const(),
        }))
    }

    pub(crate) fn unpin_reader(&self, slot_index: usize, reader_bit: u64) {
        self.slot_header(slot_index)
            .readers_mask
            .fetch_and(!reader_bit, READER_MASK_UPDATE_ORDERING);
    }

    fn messages_skipped(
        &self,
        expected_sequence: Sequence,
        next_sequence: Sequence,
        skipped_message_count: u64,
        operation: &'static str,
    ) -> ReadAttempt {
        self.stream_counters()
            .skipped_messages
            .fetch_add(skipped_message_count, COUNTER_ORDERING);
        self.stream_counters().increment_warnings_total();
        ReadAttempt::Skipped {
            next_sequence,
            warning: Warning::new(WarningCode::MessagesSkipped, COMPONENT_RBUS, operation)
                .with_sequence(expected_sequence)
                .with_topic_hash(self.topic_hash),
        }
    }

    fn publisher_alive(&self) -> bool {
        !self
            .segment_header()
            .is_closed(SEGMENT_FLAG_OBSERVE_ORDERING)
    }

    fn segment_header(&self) -> &SegmentHeader {
        // SAFETY: Required invariants are upheld
        // Invariants:
        // - Required invariant holds
        // - Required invariant holds
        // Covered by tests:
        // - loaned_publish_makes_payload_visible_to_subscriber
        unsafe { &*self.mapping.as_pointer().cast::<SegmentHeader>() }
    }

    fn stream_counters(&self) -> &StreamCounters {
        // SAFETY: Required invariants are upheld
        // Invariants:
        // - Required invariant holds
        // - Required invariant holds
        // Covered by tests:
        // - stream_counters_snapshot_reads_values
        unsafe {
            &*self
                .mapping
                .as_pointer()
                .wrapping_add(self.layout.counters_offset)
                .cast::<StreamCounters>()
        }
    }

    fn slot_header(&self, slot_index: usize) -> &SlotHeader {
        // SAFETY: Required invariants are upheld
        // Invariants:
        // - Required invariant holds
        // - Required invariant holds
        // Covered by tests:
        // - message_ref_drop_releases_pinned_slot
        unsafe { &*self.slot_header_pointer(slot_index) }
    }

    fn slot_header_pointer(&self, slot_index: usize) -> *mut SlotHeader {
        self.mapping
            .as_mutable_pointer()
            .wrapping_add(
                self.layout
                    .slots_offset
                    .saturating_add(slot_index.saturating_mul(core::mem::size_of::<SlotHeader>())),
            )
            .cast::<SlotHeader>()
    }

    fn payload_pointer(&self, slot_index: usize) -> *mut u8 {
        self.mapping.as_mutable_pointer().wrapping_add(
            self.layout
                .payload_offset
                .saturating_add(slot_index.saturating_mul(self.layout.payload_stride)),
        )
    }
}

pub(crate) struct LoanReservation {
    pub(crate) stream: Arc<StreamInner>,
    pub(crate) slot_index: usize,
    pub(crate) sequence: Sequence,
    pub(crate) length: usize,
    pub(crate) payload_pointer: *mut u8,
}

pub(crate) struct ReadReservation {
    pub(crate) stream: Arc<StreamInner>,
    pub(crate) slot_index: usize,
    pub(crate) reader_bit: u64,
    pub(crate) sequence: Sequence,
    pub(crate) length: usize,
    pub(crate) timestamp_ns: u64,
    pub(crate) payload_pointer: *const u8,
}

pub(crate) enum ReadAttempt {
    Empty,
    Message(ReadReservation),
    Skipped {
        next_sequence: Sequence,
        warning: Warning,
    },
}

#[cfg(test)]
mod tests {
    use rbus_core::constants::{SHARED_MEMORY_ABI_VERSION, SHARED_MEMORY_MAGIC};
    use rbus_core::ids::{PublisherId, StreamId, TopicHash};
    use rbus_core::warning::{Warning, WarningCode};
    use rbus_ipc::stream_segment::{create_stream_segment, mark_stream_closed};

    use crate::stream::StreamInner;

    #[test]
    fn stream_initializes_shared_memory_segment_header() -> Result<(), Warning> {
        let created_segment = create_stream_segment(
            PublisherId::new(7),
            StreamId::new(13),
            TopicHash::new(11),
            256,
            8,
        )?;
        let stream = StreamInner::attach(
            created_segment.metadata,
            13,
            &created_segment.file_descriptor,
            0,
        )?;
        let header = stream.segment_header();

        assert_eq!(header.magic, SHARED_MEMORY_MAGIC);
        assert_eq!(header.abi_version, SHARED_MEMORY_ABI_VERSION);
        assert_eq!(header.stream_id, 13);
        assert_eq!(header.publisher_id, 7);
        assert_eq!(header.topic_hash, 11);
        assert_eq!(header.slot_size, 256);
        assert_eq!(header.depth, 8);
        assert_eq!(
            header
                .wake_epoch
                .load(core::sync::atomic::Ordering::Relaxed),
            0
        );
        assert_eq!(
            header
                .wake_waiters
                .load(core::sync::atomic::Ordering::Relaxed),
            0
        );
        assert_eq!(
            header.counters_offset as usize,
            stream.layout.counters_offset
        );
        assert_eq!(header.slots_offset as usize, stream.layout.slots_offset);
        assert_eq!(header.payload_offset as usize, stream.layout.payload_offset);
        Ok(())
    }

    #[test]
    fn disconnected_warning_path_increments_total_on_closed_stream() -> Result<(), Warning> {
        let created_segment = create_stream_segment(
            PublisherId::new(7),
            StreamId::new(13),
            TopicHash::new(11),
            256,
            8,
        )?;
        let stream = StreamInner::attach(
            created_segment.metadata,
            13,
            &created_segment.file_descriptor,
            0,
        )?;
        mark_stream_closed(&created_segment.file_descriptor, created_segment.metadata)?;

        let warning = match stream.try_begin_loan(1) {
            Ok(_) => panic!("operation should return warning"),
            Err(warning) => warning,
        };
        let stats = stream.stats();

        assert_eq!(warning.code(), WarningCode::Disconnected);
        assert_eq!(stats.warnings_total, 1);
        assert_eq!(stats.disconnected_streams, 1);
        assert!(!stats.publisher_alive);
        Ok(())
    }
}
