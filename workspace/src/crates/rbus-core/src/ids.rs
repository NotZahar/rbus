//! Lightweight newtype identifiers rbus
/// Topic runtime metadata
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct TopicHash(u64);

impl TopicHash {
    /// Creates a value
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns a value
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Stream runtime metadata
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct StreamId(u64);

impl StreamId {
    /// Stream runtime metadata
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns a value
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Publisher runtime metadata
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct PublisherId(u64);

impl PublisherId {
    /// Publisher runtime metadata
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns a value
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Subscriber runtime metadata
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct SubscriberId(u8);

impl SubscriberId {
    /// Subscriber runtime metadata
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    /// Returns a value
    pub const fn as_u8(self) -> u8 {
        self.0
    }
}
