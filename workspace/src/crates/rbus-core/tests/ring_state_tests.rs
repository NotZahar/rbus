use rbus_core::constants::{MAX_SUBSCRIBERS_PER_STREAM, SLOT_WRITER_CLAIM_BIT};
use rbus_core::ids::SubscriberId;
use rbus_core::ring::pin::reader_bit_mask;
use rbus_core::ring::state::{
    decode_sequence, encode_committed_state, encode_writing_state, is_committed_state,
    is_writing_state,
};
use rbus_core::sequence::Sequence;
use rbus_core::warning::WarningCode;

const HIGHEST_READER_INDEX: u8 = MAX_SUBSCRIBERS_PER_STREAM - 1;

#[test]
fn state_sequence_roundtrip_keeps_sequence_and_writing_bit() {
    let sequence = Sequence::new(42);
    let committed_state = encode_committed_state(sequence);
    let writing_state = encode_writing_state(sequence);

    assert!(is_committed_state(committed_state));
    assert!(is_writing_state(writing_state));
    assert_eq!(decode_sequence(committed_state), sequence);
    assert_eq!(decode_sequence(writing_state), sequence);
}

#[test]
fn reader_bit_mask_uses_subscriber_id_position() {
    let mask = reader_bit_mask(SubscriberId::new(5)).expect("mask should be calculated");
    assert_eq!(mask, 1u64 << 5);
}

#[test]
fn reader_bit_mask_reserves_writer_claim_bit() {
    let highest_reader_mask =
        reader_bit_mask(SubscriberId::new(HIGHEST_READER_INDEX)).expect("mask should be valid");
    let reserved_warning = reader_bit_mask(SubscriberId::new(MAX_SUBSCRIBERS_PER_STREAM))
        .expect_err("writer claim bit should stay reserved");

    assert_ne!(highest_reader_mask, SLOT_WRITER_CLAIM_BIT);
    assert_eq!(reserved_warning.code(), WarningCode::SubscriberLimitReached);
}
