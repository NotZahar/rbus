use rbus_core::ids::SubscriberId;
use rbus_core::ring::pin::reader_bit_mask;
use rbus_core::ring::state::{
    decode_sequence, encode_committed_state, encode_writing_state, is_committed_state,
    is_writing_state,
};
use rbus_core::sequence::Sequence;

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
