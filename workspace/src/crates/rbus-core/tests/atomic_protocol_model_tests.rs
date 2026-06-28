use core::sync::atomic::Ordering;

use loom::sync::Arc;
use loom::sync::atomic::AtomicU64;
use loom::thread;
use rbus_core::Sequence;
use rbus_core::atomic::ordering::{
    READER_MASK_UPDATE_ORDERING, READER_OBSERVE_ORDERING, READER_PIN_FAILURE_ORDERING,
    READER_PIN_ORDERING, READER_SLOT_OBSERVE_ORDERING, WRITER_CLAIM_FAILURE_ORDERING,
    WRITER_CLAIM_ORDERING, WRITER_COMMIT_ORDERING, WRITER_STATE_UPDATE_ORDERING,
};
use rbus_core::constants::SLOT_WRITER_CLAIM_BIT;
use rbus_core::ring::state::{encode_committed_state, encode_writing_state};

const EMPTY_READER_MASK: u64 = 0;
const FIRST_READER_BIT: u64 = 1;
const FIRST_SEQUENCE_VALUE: u64 = 0;
const NEXT_SEQUENCE_VALUE: u64 = 1;
const PUBLISHED_LENGTH: u64 = 8;
const PUBLISHED_PAYLOAD_WORD: u64 = 0x5242_5553_5041_594c;
const SINGLE_READER_BIT_CLEAR: u64 = 1;
const PAYLOAD_METADATA_ORDERING: Ordering = Ordering::Relaxed;

struct ModelSlot {
    state_sequence: AtomicU64,
    readers_mask: AtomicU64,
    write_sequence: AtomicU64,
    published_length: AtomicU64,
    payload_word: AtomicU64,
}

impl ModelSlot {
    fn committed(sequence: Sequence, readers_mask: u64) -> Self {
        Self {
            state_sequence: AtomicU64::new(encode_committed_state(sequence)),
            readers_mask: AtomicU64::new(readers_mask),
            write_sequence: AtomicU64::new(sequence.next().as_u64()),
            published_length: AtomicU64::new(EMPTY_READER_MASK),
            payload_word: AtomicU64::new(EMPTY_READER_MASK),
        }
    }

    fn writing(sequence: Sequence) -> Self {
        Self {
            state_sequence: AtomicU64::new(encode_writing_state(sequence)),
            readers_mask: AtomicU64::new(SLOT_WRITER_CLAIM_BIT),
            write_sequence: AtomicU64::new(sequence.as_u64()),
            published_length: AtomicU64::new(EMPTY_READER_MASK),
            payload_word: AtomicU64::new(EMPTY_READER_MASK),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReaderPinOutcome {
    NotAvailable,
    Pinned,
    RepinDetected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WriterClaimOutcome {
    BlockedByReader,
    Claimed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ObservedMessage {
    length: u64,
    payload_word: u64,
}

#[test]
fn slot_protocol_orderings_do_not_require_global_seq_cst() {
    assert_ne!(WRITER_CLAIM_ORDERING, Ordering::SeqCst);
    assert_ne!(WRITER_CLAIM_FAILURE_ORDERING, Ordering::SeqCst);
    assert_ne!(WRITER_STATE_UPDATE_ORDERING, Ordering::SeqCst);
    assert_ne!(WRITER_COMMIT_ORDERING, Ordering::SeqCst);
    assert_ne!(READER_SLOT_OBSERVE_ORDERING, Ordering::SeqCst);
    assert_ne!(READER_PIN_ORDERING, Ordering::SeqCst);
    assert_ne!(READER_PIN_FAILURE_ORDERING, Ordering::SeqCst);
    assert_ne!(READER_MASK_UPDATE_ORDERING, Ordering::SeqCst);
}

#[test]
fn reader_pin_and_writer_overwrite_are_mutually_exclusive() {
    loom::model(|| {
        let reader_sequence = Sequence::new(FIRST_SEQUENCE_VALUE);
        let writer_sequence = Sequence::new(NEXT_SEQUENCE_VALUE);
        let slot = Arc::new(ModelSlot::committed(reader_sequence, EMPTY_READER_MASK));
        let reader_slot = Arc::clone(&slot);
        let writer_slot = Arc::clone(&slot);

        let reader =
            thread::spawn(move || try_pin_reader(&reader_slot, reader_sequence, FIRST_READER_BIT));
        let writer = thread::spawn(move || try_claim_writer(&writer_slot, writer_sequence));

        let reader_outcome = reader.join().expect("reader model thread should finish");
        let writer_outcome = writer.join().expect("writer model thread should finish");

        assert_ne!(
            (reader_outcome, writer_outcome),
            (ReaderPinOutcome::Pinned, WriterClaimOutcome::Claimed)
        );
    });
}

#[test]
fn commit_release_is_visible_after_observe_acquire() {
    loom::model(|| {
        let sequence = Sequence::new(FIRST_SEQUENCE_VALUE);
        let slot = Arc::new(ModelSlot::writing(sequence));
        let writer_slot = Arc::clone(&slot);
        let reader_slot = Arc::clone(&slot);

        let writer = thread::spawn(move || commit_message(&writer_slot, sequence));
        let reader = thread::spawn(move || observe_message(&reader_slot, sequence));

        writer.join().expect("writer model thread should finish");
        if let Some(observed_message) = reader.join().expect("reader model thread should finish") {
            assert_eq!(observed_message.length, PUBLISHED_LENGTH);
            assert_eq!(observed_message.payload_word, PUBLISHED_PAYLOAD_WORD);
        }
    });
}

#[test]
fn cleanup_and_reader_drop_clear_reader_bit_once() {
    loom::model(|| {
        let sequence = Sequence::new(FIRST_SEQUENCE_VALUE);
        let slot = Arc::new(ModelSlot::committed(sequence, FIRST_READER_BIT));
        let cleanup_slot = Arc::clone(&slot);
        let reader_drop_slot = Arc::clone(&slot);

        let cleanup = thread::spawn(move || clear_reader_bit(&cleanup_slot, FIRST_READER_BIT));
        let reader_drop =
            thread::spawn(move || clear_reader_bit(&reader_drop_slot, FIRST_READER_BIT));

        let cleanup_cleared = cleanup.join().expect("cleanup model thread should finish");
        let reader_drop_cleared = reader_drop
            .join()
            .expect("reader drop model thread should finish");
        let final_reader_mask = slot.readers_mask.load(READER_OBSERVE_ORDERING);

        assert_eq!(final_reader_mask & FIRST_READER_BIT, EMPTY_READER_MASK);
        assert_eq!(
            clear_count(cleanup_cleared) + clear_count(reader_drop_cleared),
            SINGLE_READER_BIT_CLEAR
        );
    });
}

fn try_pin_reader(
    slot: &ModelSlot,
    expected_sequence: Sequence,
    reader_bit: u64,
) -> ReaderPinOutcome {
    let state_sequence = slot.state_sequence.load(READER_SLOT_OBSERVE_ORDERING);
    if state_sequence != encode_committed_state(expected_sequence) {
        return ReaderPinOutcome::NotAvailable;
    }

    let mut readers_mask = slot.readers_mask.load(READER_SLOT_OBSERVE_ORDERING);
    loop {
        if readers_mask & SLOT_WRITER_CLAIM_BIT != EMPTY_READER_MASK {
            return ReaderPinOutcome::NotAvailable;
        }
        match slot.readers_mask.compare_exchange(
            readers_mask,
            readers_mask | reader_bit,
            READER_PIN_ORDERING,
            READER_PIN_FAILURE_ORDERING,
        ) {
            Ok(_) => break,
            Err(observed_readers_mask) => readers_mask = observed_readers_mask,
        }
    }
    let state_after_pin = slot.state_sequence.load(READER_SLOT_OBSERVE_ORDERING);
    if state_after_pin == state_sequence {
        ReaderPinOutcome::Pinned
    } else {
        slot.readers_mask
            .fetch_and(!reader_bit, READER_MASK_UPDATE_ORDERING);
        ReaderPinOutcome::RepinDetected
    }
}

fn try_claim_writer(slot: &ModelSlot, sequence: Sequence) -> WriterClaimOutcome {
    let claimed = slot
        .readers_mask
        .compare_exchange(
            EMPTY_READER_MASK,
            SLOT_WRITER_CLAIM_BIT,
            WRITER_CLAIM_ORDERING,
            WRITER_CLAIM_FAILURE_ORDERING,
        )
        .is_ok();
    if !claimed {
        return WriterClaimOutcome::BlockedByReader;
    }

    slot.state_sequence
        .store(encode_writing_state(sequence), WRITER_STATE_UPDATE_ORDERING);
    WriterClaimOutcome::Claimed
}

fn commit_message(slot: &ModelSlot, sequence: Sequence) {
    slot.payload_word
        .store(PUBLISHED_PAYLOAD_WORD, PAYLOAD_METADATA_ORDERING);
    slot.published_length
        .store(PUBLISHED_LENGTH, PAYLOAD_METADATA_ORDERING);
    slot.state_sequence.store(
        encode_committed_state(sequence),
        WRITER_STATE_UPDATE_ORDERING,
    );
    slot.write_sequence
        .store(sequence.next().as_u64(), WRITER_COMMIT_ORDERING);
    slot.readers_mask
        .fetch_and(!SLOT_WRITER_CLAIM_BIT, READER_MASK_UPDATE_ORDERING);
}

fn observe_message(slot: &ModelSlot, expected_sequence: Sequence) -> Option<ObservedMessage> {
    let current_write_sequence = Sequence::new(slot.write_sequence.load(READER_OBSERVE_ORDERING));
    if expected_sequence >= current_write_sequence {
        return None;
    }

    let state_sequence = slot.state_sequence.load(READER_OBSERVE_ORDERING);
    if state_sequence != encode_committed_state(expected_sequence) {
        return None;
    }

    Some(ObservedMessage {
        length: slot.published_length.load(PAYLOAD_METADATA_ORDERING),
        payload_word: slot.payload_word.load(PAYLOAD_METADATA_ORDERING),
    })
}

fn clear_reader_bit(slot: &ModelSlot, reader_bit: u64) -> bool {
    let previous_mask = slot
        .readers_mask
        .fetch_and(!reader_bit, READER_MASK_UPDATE_ORDERING);
    previous_mask & reader_bit != EMPTY_READER_MASK
}

fn clear_count(cleared: bool) -> u64 {
    u64::from(u8::from(cleared))
}
