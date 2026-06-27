//! Encoding `state_seq`: sequence plus writing bit
use crate::sequence::Sequence;

const WRITING_BIT: u64 = 1;

/// Internal documentation
pub const fn encode_committed_state(sequence: Sequence) -> u64 {
    sequence.as_u64() << 1
}

/// Internal documentation
pub const fn encode_writing_state(sequence: Sequence) -> u64 {
    (sequence.as_u64() << 1) | WRITING_BIT
}

/// Internal documentation
pub const fn is_writing_state(state_sequence: u64) -> bool {
    (state_sequence & WRITING_BIT) == WRITING_BIT
}

/// Internal documentation
pub const fn is_committed_state(state_sequence: u64) -> bool {
    !is_writing_state(state_sequence)
}

/// Sequence metadata
pub const fn decode_sequence(state_sequence: u64) -> Sequence {
    Sequence::new(state_sequence >> 1)
}
