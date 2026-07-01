//! Per-stream subscriber read cursor

use crate::sequence::Sequence;

/// Expected sequence for the next read from a stream
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamCursor {
    expected_sequence: Sequence,
}

impl StreamCursor {
    /// Creates a cursor expecting the provided sequence
    pub const fn new(expected_sequence: Sequence) -> Self {
        Self { expected_sequence }
    }

    /// Returns the next sequence this cursor expects to read
    pub const fn expected_sequence(self) -> Sequence {
        self.expected_sequence
    }

    /// Advances the cursor by one sequence
    pub fn advance(&mut self) {
        self.expected_sequence = self.expected_sequence.next();
    }

    /// Moves the cursor to an explicit sequence
    pub fn move_to(&mut self, expected_sequence: Sequence) {
        self.expected_sequence = expected_sequence;
    }
}
