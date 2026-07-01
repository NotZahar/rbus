//! Monotonic message sequence numbers

/// Sequence number assigned to messages in one stream
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Sequence(u64);

impl Sequence {
    /// Creates a sequence number from its raw encoded value
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the raw encoded sequence number
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Returns the next sequence number, saturating at `u64::MAX`
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }

    /// Maps this sequence to a ring slot for a power-of-two depth
    pub const fn slot_index(self, depth: usize) -> usize {
        (self.0 as usize) & (depth - 1)
    }

    /// Counts messages skipped between this cursor and an observed sequence
    pub fn skipped_messages_before(self, observed_sequence: Self) -> u64 {
        observed_sequence.0.saturating_sub(self.0)
    }
}
