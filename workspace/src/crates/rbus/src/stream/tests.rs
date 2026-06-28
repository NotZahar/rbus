use std::sync::Arc;
use std::sync::atomic::Ordering;

use rbus_core::constants::{
    SLOT_WRITER_CLAIM_BIT, STREAM_SEGMENT_ABI_VERSION, STREAM_SEGMENT_MAGIC,
};
use rbus_core::ids::{PublisherId, StreamId, SubscriberId, TopicHash};
use rbus_core::ring::pin::reader_bit_mask;
use rbus_core::sequence::Sequence;
use rbus_core::warning::{Warning, WarningCode};
use rbus_ipc::stream_segment::{create_stream_segment, mark_stream_closed};

use crate::stream::{ReadAttempt, StreamInner};

const PUBLISHER_ID: PublisherId = PublisherId::new(7);
const STREAM_ID: StreamId = StreamId::new(13);
const TOPIC_HASH: TopicHash = TopicHash::new(11);
const CREATED_GENERATION: u64 = STREAM_ID.as_u64();
const MAX_MESSAGE_SIZE: usize = 256;
const STREAM_DEPTH: usize = 8;
const INITIAL_SUBSCRIBER_COUNT: u64 = 0;
const MESSAGE_LENGTH: usize = 1;
const SUBSCRIBER_ID: SubscriberId = SubscriberId::new(1);
const OTHER_SUBSCRIBER_ID: SubscriberId = SubscriberId::new(2);
const EMPTY_READERS_MASK: u64 = 0;
const FIRST_PUBLISHED_MESSAGE_COUNT: u64 = 1;

#[test]
fn stream_initializes_shared_memory_segment_header() -> Result<(), Warning> {
    let created_segment = create_test_segment()?;
    let stream = attach_test_stream(&created_segment)?;
    let header = stream.segment_header();

    assert_eq!(header.magic, STREAM_SEGMENT_MAGIC);
    assert_eq!(header.abi_version, STREAM_SEGMENT_ABI_VERSION);
    assert_eq!(header.stream_id, STREAM_ID.as_u64());
    assert_eq!(header.publisher_id, PUBLISHER_ID.as_u64());
    assert_eq!(header.topic_hash, TOPIC_HASH.as_u64());
    assert_eq!(header.slot_size, MAX_MESSAGE_SIZE as u64);
    assert_eq!(header.depth, STREAM_DEPTH as u64);
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
    let created_segment = create_test_segment()?;
    let stream = attach_test_stream(&created_segment)?;
    mark_stream_closed(&created_segment.file_descriptor, created_segment.metadata)?;

    let warning = match stream.try_begin_loan(MESSAGE_LENGTH) {
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

#[test]
fn message_reservations_do_not_clone_stream_arc() -> Result<(), Warning> {
    let created_segment = create_test_segment()?;
    let stream = attach_test_stream(&created_segment)?;
    let initial_strong_count = Arc::strong_count(&stream);

    let loan_reservation = stream.try_begin_loan(MESSAGE_LENGTH)?;
    assert_eq!(Arc::strong_count(&stream), initial_strong_count);

    stream.commit_loan(
        loan_reservation.slot_index,
        loan_reservation.sequence,
        loan_reservation.length,
    )?;

    let read_reservation =
        match stream.try_read_message(loan_reservation.sequence, SUBSCRIBER_ID)? {
            ReadAttempt::Message(read_reservation) => read_reservation,
            ReadAttempt::Empty => panic!("message should be available"),
            ReadAttempt::Skipped { .. } => panic!("message should not be skipped"),
        };
    assert_eq!(Arc::strong_count(&stream), initial_strong_count);

    // SAFETY: The test stream Arc is still alive and owns the mapped StreamInner.
    let stream_reference = unsafe { read_reservation.stream.as_ref() };
    stream_reference.unpin_reader(read_reservation.slot_index, read_reservation.reader_bit);
    assert_eq!(Arc::strong_count(&stream), initial_strong_count);
    Ok(())
}

#[test]
fn uncontended_reader_pin_uses_empty_mask_path() -> Result<(), Warning> {
    let created_segment = create_test_segment()?;
    let stream = attach_test_stream(&created_segment)?;
    let (sequence, slot_index) = publish_test_message(&stream)?;

    assert_eq!(
        stream
            .slot_header(slot_index)
            .readers_mask
            .load(Ordering::Relaxed),
        EMPTY_READERS_MASK
    );
    assert_eq!(
        stream.stats().published_messages,
        FIRST_PUBLISHED_MESSAGE_COUNT
    );

    let read_reservation = read_test_message(&stream, sequence, SUBSCRIBER_ID)?;
    let reader_bit = reader_bit_mask(SUBSCRIBER_ID)?;
    assert_eq!(
        stream
            .slot_header(read_reservation.slot_index)
            .readers_mask
            .load(Ordering::Relaxed),
        reader_bit
    );

    stream.unpin_reader(read_reservation.slot_index, read_reservation.reader_bit);
    assert_eq!(
        stream
            .slot_header(slot_index)
            .readers_mask
            .load(Ordering::Relaxed),
        EMPTY_READERS_MASK
    );
    Ok(())
}

#[test]
fn contended_reader_pin_keeps_multi_subscriber_fanout() -> Result<(), Warning> {
    let created_segment = create_test_segment()?;
    let stream = attach_test_stream(&created_segment)?;
    let (sequence, slot_index) = publish_test_message(&stream)?;
    let first_reader_bit = reader_bit_mask(SUBSCRIBER_ID)?;
    let second_reader_bit = reader_bit_mask(OTHER_SUBSCRIBER_ID)?;
    stream
        .slot_header(slot_index)
        .readers_mask
        .store(first_reader_bit, Ordering::Relaxed);

    let read_reservation = read_test_message(&stream, sequence, OTHER_SUBSCRIBER_ID)?;
    assert_eq!(
        stream
            .slot_header(slot_index)
            .readers_mask
            .load(Ordering::Relaxed),
        first_reader_bit | second_reader_bit
    );

    stream.unpin_reader(read_reservation.slot_index, read_reservation.reader_bit);
    stream.unpin_reader(slot_index, first_reader_bit);
    assert_eq!(
        stream
            .slot_header(slot_index)
            .readers_mask
            .load(Ordering::Relaxed),
        EMPTY_READERS_MASK
    );
    Ok(())
}

#[test]
fn rollback_clears_writer_claim_without_reader_bits() -> Result<(), Warning> {
    let created_segment = create_test_segment()?;
    let stream = attach_test_stream(&created_segment)?;
    let loan_reservation = stream.try_begin_loan(MESSAGE_LENGTH)?;
    assert_eq!(
        stream
            .slot_header(loan_reservation.slot_index)
            .readers_mask
            .load(Ordering::Relaxed),
        SLOT_WRITER_CLAIM_BIT
    );

    stream.rollback_loan(loan_reservation.slot_index);
    assert_eq!(
        stream
            .slot_header(loan_reservation.slot_index)
            .readers_mask
            .load(Ordering::Relaxed),
        EMPTY_READERS_MASK
    );
    Ok(())
}

fn publish_test_message(stream: &StreamInner) -> Result<(Sequence, usize), Warning> {
    let loan_reservation = stream.try_begin_loan(MESSAGE_LENGTH)?;
    assert_eq!(
        stream
            .slot_header(loan_reservation.slot_index)
            .readers_mask
            .load(Ordering::Relaxed),
        SLOT_WRITER_CLAIM_BIT
    );
    stream.commit_loan(
        loan_reservation.slot_index,
        loan_reservation.sequence,
        loan_reservation.length,
    )?;
    Ok((loan_reservation.sequence, loan_reservation.slot_index))
}

fn read_test_message(
    stream: &StreamInner,
    sequence: Sequence,
    subscriber_id: SubscriberId,
) -> Result<crate::stream::ReadReservation<'_>, Warning> {
    match stream.try_read_message(sequence, subscriber_id)? {
        ReadAttempt::Message(read_reservation) => Ok(read_reservation),
        ReadAttempt::Empty => panic!("message should be available"),
        ReadAttempt::Skipped { .. } => panic!("message should not be skipped"),
    }
}

fn create_test_segment() -> Result<rbus_ipc::stream_segment::CreatedStreamSegment, Warning> {
    create_stream_segment(
        PUBLISHER_ID,
        STREAM_ID,
        TOPIC_HASH,
        MAX_MESSAGE_SIZE,
        STREAM_DEPTH,
    )
}

fn attach_test_stream(
    created_segment: &rbus_ipc::stream_segment::CreatedStreamSegment,
) -> Result<Arc<StreamInner>, Warning> {
    StreamInner::attach(
        created_segment.metadata,
        CREATED_GENERATION,
        &created_segment.file_descriptor,
        INITIAL_SUBSCRIBER_COUNT,
    )
}
