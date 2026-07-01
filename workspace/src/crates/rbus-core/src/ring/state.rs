//! Encoding `state_seq`: sequence plus writing bit
use crate::sequence::Sequence;

const WRITING_BIT: u64 = 1;

/// Encodes a committed slot state for the given sequence
pub const fn encode_committed_state(sequence: Sequence) -> u64 {
    sequence.as_u64() << 1
}

/// Encodes a transient writer-owned slot state for the given sequence
pub const fn encode_writing_state(sequence: Sequence) -> u64 {
    (sequence.as_u64() << 1) | WRITING_BIT
}

/// Returns true if the encoded state has the writer bit set
pub const fn is_writing_state(state_sequence: u64) -> bool {
    (state_sequence & WRITING_BIT) == WRITING_BIT
}

/// Returns true if the encoded state is visible to readers
pub const fn is_committed_state(state_sequence: u64) -> bool {
    !is_writing_state(state_sequence)
}

/// Extracts the message sequence from an encoded slot state
pub const fn decode_sequence(state_sequence: u64) -> Sequence {
    Sequence::new(state_sequence >> 1)
}
