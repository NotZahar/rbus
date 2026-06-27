//! Publisher runtime metadata

/// Stream runtime metadata
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Sequence(u64);

impl Sequence {
    /// Sequence metadata
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns a value
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Sequence metadata
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }

    /// Slot metadata
    pub const fn slot_index(self, depth: usize) -> usize {
        (self.0 as usize) & (depth - 1)
    }

    /// Sequence metadata
    pub fn skipped_messages_before(self, observed_sequence: Self) -> u64 {
        observed_sequence.0.saturating_sub(self.0)
    }
}
