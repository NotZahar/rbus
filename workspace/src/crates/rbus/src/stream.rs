use core::marker::PhantomData;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rbus_core::atomic::ordering::{
    COUNTER_ORDERING, READER_MASK_UPDATE_ORDERING, READER_OBSERVE_ORDERING,
    READER_PIN_FAILURE_ORDERING, READER_PIN_ORDERING, READER_SLOT_OBSERVE_ORDERING,
    SEGMENT_FLAG_OBSERVE_ORDERING, WRITER_CLAIM_CLEAR_ORDERING, WRITER_CLAIM_FAILURE_ORDERING,
    WRITER_CLAIM_ORDERING, WRITER_COMMIT_ORDERING, WRITER_STATE_UPDATE_ORDERING,
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
const SEGMENT_HEADER_OFFSET: usize = 0;
const COUNTER_INCREMENT: u64 = 1;

pub(crate) struct StreamInner {
    stream_id: StreamId,
    publisher_id: PublisherId,
    topic_hash: TopicHash,
    created_generation: u64,
    layout: SegmentLayout,
    memory: StreamMemory,
    _mapping: MappedRegion,
    subscriber_count: core::sync::atomic::AtomicU64,
    published_messages: AtomicU64,
}

impl StreamInner {
    pub(crate) fn attach(
        metadata: StreamSegmentMetadata,
        created_generation: u64,
        file_descriptor: &OwnedFileDescriptor,
        initial_subscriber_count: u64,
    ) -> Result<Arc<Self>, Warning> {
        let (mapping, layout) = map_stream_segment(file_descriptor, metadata, true)?;
        let memory = StreamMemory::new(&mapping, layout)?;
        // SAFETY: StreamMemory::new checked the counters pointer before StreamInner creation
        let published_messages = unsafe {
            memory
                .counters
                .as_ref()
                .published_messages
                .load(COUNTER_ORDERING)
        };

        Ok(Arc::new(Self {
            stream_id: metadata.stream_id,
            publisher_id: metadata.publisher_id,
            topic_hash: metadata.topic_hash,
            created_generation,
            layout,
            memory,
            _mapping: mapping,
            subscriber_count: core::sync::atomic::AtomicU64::new(initial_subscriber_count),
            published_messages: AtomicU64::new(published_messages),
        }))
    }

    pub(crate) const fn stream_id(&self) -> StreamId {
        self.stream_id
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

    pub(crate) fn try_begin_loan(&self, length: usize) -> Result<LoanReservation, Warning> {
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

        let sequence = Sequence::new(self.segment_header().write_sequence.load(Ordering::Relaxed));
        let slot_index = sequence.slot_index(self.layout.depth);
        let slot = self.slot_header(slot_index);
        if slot
            .readers_mask
            .compare_exchange(
                0,
                SLOT_WRITER_CLAIM_BIT,
                WRITER_CLAIM_ORDERING,
                WRITER_CLAIM_FAILURE_ORDERING,
            )
            .is_err()
        {
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
            .store(encode_writing_state(sequence), WRITER_STATE_UPDATE_ORDERING);

        Ok(LoanReservation {
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
        // SAFETY: The slot was claimed by try_begin_loan, slot_index points inside the checked
        // header range, and readers cannot observe this metadata before the release commit
        unsafe {
            (*slot_pointer).len = length as u32;
            (*slot_pointer).flags = 0;
            (*slot_pointer).timestamp_ns = timestamp_ns;
        }
        self.slot_header(slot_index).state_seq.store(
            encode_committed_state(sequence),
            WRITER_STATE_UPDATE_ORDERING,
        );
        self.segment_header()
            .write_sequence
            .store(sequence.next().as_u64(), WRITER_COMMIT_ORDERING);
        self.slot_header(slot_index)
            .readers_mask
            .store(0, WRITER_CLAIM_CLEAR_ORDERING);
        self.increment_published_messages();
        Ok(())
    }

    pub(crate) fn rollback_loan(&self, slot_index: usize) {
        let slot_pointer = self.slot_header_pointer(slot_index);
        // SAFETY: The slot was claimed by try_begin_loan and slot_index points inside the
        // checked header range for this stream layout
        unsafe {
            (*slot_pointer).len = 0;
        }
        self.slot_header(slot_index)
            .state_seq
            .store(0, WRITER_STATE_UPDATE_ORDERING);
        self.slot_header(slot_index)
            .readers_mask
            .store(0, WRITER_CLAIM_CLEAR_ORDERING);
    }

    pub(crate) fn try_read_message(
        &self,
        expected_sequence: Sequence,
        subscriber_id: SubscriberId,
    ) -> Result<ReadAttempt<'_>, Warning> {
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
        let state_sequence = slot.state_seq.load(READER_SLOT_OBSERVE_ORDERING);
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
        if !self.pin_reader(slot, reader_bit) {
            return Ok(ReadAttempt::Empty);
        }

        let state_after_pin = slot.state_seq.load(READER_SLOT_OBSERVE_ORDERING);
        if state_after_pin != state_sequence {
            slot.readers_mask
                .fetch_and(!reader_bit, READER_MASK_UPDATE_ORDERING);
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
            stream: NonNull::from(self),
            _stream_lifetime: PhantomData,
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

    fn pin_reader(&self, slot: &SlotHeader, reader_bit: u64) -> bool {
        match slot.readers_mask.compare_exchange(
            0,
            reader_bit,
            READER_PIN_ORDERING,
            READER_PIN_FAILURE_ORDERING,
        ) {
            Ok(_) => true,
            Err(readers_mask) => pin_reader_after_contention(slot, reader_bit, readers_mask),
        }
    }

    fn increment_published_messages(&self) {
        let published_messages = self
            .published_messages
            .load(COUNTER_ORDERING)
            .wrapping_add(COUNTER_INCREMENT);
        self.published_messages
            .store(published_messages, COUNTER_ORDERING);
        self.stream_counters()
            .published_messages
            .store(published_messages, COUNTER_ORDERING);
    }

    fn messages_skipped(
        &self,
        expected_sequence: Sequence,
        next_sequence: Sequence,
        skipped_message_count: u64,
        operation: &'static str,
    ) -> ReadAttempt<'_> {
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
        // SAFETY: The pointer was checked once during StreamInner attach
        unsafe { self.memory.header.as_ref() }
    }

    fn stream_counters(&self) -> &StreamCounters {
        // SAFETY: The pointer was checked once during StreamInner attach
        unsafe { self.memory.counters.as_ref() }
    }

    fn slot_header(&self, slot_index: usize) -> &SlotHeader {
        // SAFETY: Callers derive slot_index from Sequence::slot_index or a bounded layout loop
        unsafe { &*self.slot_header_pointer(slot_index) }
    }

    fn slot_header_pointer(&self, slot_index: usize) -> *mut SlotHeader {
        // SAFETY: StreamMemory::new checked the full slot header range for this layout
        unsafe { self.memory.slots.as_ptr().add(slot_index) }
    }

    fn payload_pointer(&self, slot_index: usize) -> *mut u8 {
        let payload_offset = slot_index * self.layout.payload_stride;
        // SAFETY: StreamMemory::new checked the full payload range for this layout
        unsafe { self.memory.payload.as_ptr().add(payload_offset) }
    }
}

fn pin_reader_after_contention(slot: &SlotHeader, reader_bit: u64, mut readers_mask: u64) -> bool {
    loop {
        if readers_mask & SLOT_WRITER_CLAIM_BIT != 0 {
            return false;
        }
        let pinned_readers_mask = readers_mask | reader_bit;
        if pinned_readers_mask == readers_mask {
            return true;
        }
        match slot.readers_mask.compare_exchange(
            readers_mask,
            pinned_readers_mask,
            READER_PIN_ORDERING,
            READER_PIN_FAILURE_ORDERING,
        ) {
            Ok(_) => return true,
            Err(observed_readers_mask) => readers_mask = observed_readers_mask,
        }
    }
}

struct StreamMemory {
    header: NonNull<SegmentHeader>,
    counters: NonNull<StreamCounters>,
    slots: NonNull<SlotHeader>,
    payload: NonNull<u8>,
}

// SAFETY: StreamMemory pointers are checked during attach, remain valid while StreamInner owns
// the MappedRegion, and concurrent access is synchronized by the shared-memory atomics
unsafe impl Send for StreamMemory {}

// SAFETY: StreamMemory exposes shared-memory metadata and payload pointers whose concurrent access
// is governed by the slot protocol atomics, not by Rust references
unsafe impl Sync for StreamMemory {}

impl StreamMemory {
    fn new(mapping: &MappedRegion, layout: SegmentLayout) -> Result<Self, Warning> {
        let slot_headers_size = checked_region_size(
            core::mem::size_of::<SlotHeader>(),
            layout.depth,
            "StreamMemory::slot_headers_size",
        )?;
        let payload_size = checked_region_size(
            layout.payload_stride,
            layout.depth,
            "StreamMemory::payload_size",
        )?;
        let header = mapping
            .typed_pointer::<SegmentHeader>(SEGMENT_HEADER_OFFSET, "StreamMemory::header")?;
        let counters = mapping
            .typed_pointer::<StreamCounters>(layout.counters_offset, "StreamMemory::counters")?;
        let slots =
            mapping.typed_pointer::<SlotHeader>(layout.slots_offset, "StreamMemory::slots")?;
        mapping.byte_pointer(
            layout.slots_offset,
            slot_headers_size,
            "StreamMemory::slots_range",
        )?;
        let payload = mapping.byte_pointer(
            layout.payload_offset,
            payload_size,
            "StreamMemory::payload_range",
        )?;
        Ok(Self {
            header,
            counters,
            slots,
            payload,
        })
    }
}

fn checked_region_size(
    element_size: usize,
    element_count: usize,
    operation: &'static str,
) -> Result<usize, Warning> {
    element_size
        .checked_mul(element_count)
        .ok_or_else(|| Warning::new(WarningCode::ResourceLimit, COMPONENT_RBUS, operation))
}

pub(crate) struct LoanReservation {
    pub(crate) slot_index: usize,
    pub(crate) sequence: Sequence,
    pub(crate) length: usize,
    pub(crate) payload_pointer: *mut u8,
}

pub(crate) struct ReadReservation<'stream> {
    pub(crate) stream: NonNull<StreamInner>,
    pub(crate) _stream_lifetime: PhantomData<&'stream StreamInner>,
    pub(crate) slot_index: usize,
    pub(crate) reader_bit: u64,
    pub(crate) sequence: Sequence,
    pub(crate) length: usize,
    pub(crate) timestamp_ns: u64,
    pub(crate) payload_pointer: *const u8,
}

pub(crate) enum ReadAttempt<'stream> {
    Empty,
    Message(ReadReservation<'stream>),
    Skipped {
        next_sequence: Sequence,
        warning: Warning,
    },
}

#[cfg(test)]
mod tests;
