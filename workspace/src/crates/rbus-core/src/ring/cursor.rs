//! Subscriber runtime metadata

use crate::sequence::Sequence;

/// Subscriber runtime metadata
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamCursor {
    expected_sequence: Sequence,
}

impl StreamCursor {
    /// Sequence metadata
    pub const fn new(expected_sequence: Sequence) -> Self {
        Self { expected_sequence }
    }

    /// Sequence metadata
    pub const fn expected_sequence(self) -> Sequence {
        self.expected_sequence
    }

    /// Sequence metadata
    pub fn advance(&mut self) {
        self.expected_sequence = self.expected_sequence.next();
    }

    /// Sequence metadata
    pub fn move_to(&mut self, expected_sequence: Sequence) {
        self.expected_sequence = expected_sequence;
    }
}
