//! Lightweight newtype identifiers used across control-plane and data-plane code

/// Stable hash that identifies a topic in shared-memory and control messages
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct TopicHash(u64);

impl TopicHash {
    /// Creates a topic hash from its raw encoded value
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the raw encoded topic hash
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Daemon-assigned identifier for one publisher stream
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct StreamId(u64);

impl StreamId {
    /// Creates a stream identifier from its raw encoded value
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the raw encoded stream identifier
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Daemon-assigned identifier for a publisher session
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct PublisherId(u64);

impl PublisherId {
    /// Creates a publisher identifier from its raw encoded value
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the raw encoded publisher identifier
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Per-stream reader index assigned to a subscriber
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct SubscriberId(u8);

impl SubscriberId {
    /// Creates a subscriber identifier from its raw encoded value
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    /// Returns the raw encoded subscriber identifier
    pub const fn as_u8(self) -> u8 {
        self.0
    }
}
