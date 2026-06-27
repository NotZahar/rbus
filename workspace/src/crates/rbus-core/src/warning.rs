//! Warning model metadata

use crate::ids::{StreamId, TopicHash};
use crate::sequence::Sequence;

const HAS_OS_ERRNO: u8 = 1 << 0;
const HAS_SEQUENCE: u8 = 1 << 1;
const HAS_TOPIC_HASH: u8 = 1 << 2;
const HAS_STREAM_ID: u8 = 1 << 3;
const HAS_MONOTONIC_TIMESTAMP_NS: u8 = 1 << 4;

/// Internal documentation
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum WarningCode {
    /// Daemon runtime metadata
    DaemonUnavailable,
    /// Internal documentation
    ProtocolMismatch,
    /// Internal documentation
    PermissionDenied,
    /// Internal documentation
    SyscallFailed,
    /// Linux IPC metadata
    MmapFailed,
    /// Internal documentation
    InvalidConfig,
    /// Slot metadata
    MessageTooLarge,
    /// Slot metadata
    BackpressurePinnedSlot,
    /// Subscriber runtime metadata
    MessagesSkipped,
    /// Internal documentation
    Timeout,
    /// Stream runtime metadata
    Disconnected,
    /// Shared memory ABI metadata
    SegmentAbiMismatch,
    /// Internal documentation
    SegmentQuarantined,
    /// Subscriber runtime metadata
    SubscriberLimitReached,
    /// Internal documentation
    ResourceLimit,
    /// Internal documentation
    CleanupIncomplete,
    /// Internal documentation
    InternalInvariantWarning,
}

impl WarningCode {
    /// Returns a value
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DaemonUnavailable => "DaemonUnavailable",
            Self::ProtocolMismatch => "ProtocolMismatch",
            Self::PermissionDenied => "PermissionDenied",
            Self::SyscallFailed => "SyscallFailed",
            Self::MmapFailed => "MmapFailed",
            Self::InvalidConfig => "InvalidConfig",
            Self::MessageTooLarge => "MessageTooLarge",
            Self::BackpressurePinnedSlot => "BackpressurePinnedSlot",
            Self::MessagesSkipped => "MessagesSkipped",
            Self::Timeout => "Timeout",
            Self::Disconnected => "Disconnected",
            Self::SegmentAbiMismatch => "SegmentAbiMismatch",
            Self::SegmentQuarantined => "SegmentQuarantined",
            Self::SubscriberLimitReached => "SubscriberLimitReached",
            Self::ResourceLimit => "ResourceLimit",
            Self::CleanupIncomplete => "CleanupIncomplete",
            Self::InternalInvariantWarning => "InternalInvariantWarning",
        }
    }

    /// Returns a value
    pub const fn description(self) -> &'static str {
        match self {
            Self::DaemonUnavailable => "rbusd daemon is unavailable",
            Self::ProtocolMismatch => "protocol version mismatch",
            Self::PermissionDenied => "permission denied",
            Self::SyscallFailed => "syscall failed",
            Self::MmapFailed => "mmap failed",
            Self::InvalidConfig => "invalid configuration",
            Self::MessageTooLarge => "message exceeds slot size",
            Self::BackpressurePinnedSlot => "slot is pinned by a reader",
            Self::MessagesSkipped => "subscriber skipped messages",
            Self::Timeout => "wait timeout expired",
            Self::Disconnected => "connection or stream is disconnected",
            Self::SegmentAbiMismatch => "segment ABI mismatch",
            Self::SegmentQuarantined => "segment is quarantined",
            Self::SubscriberLimitReached => "subscriber limit reached",
            Self::ResourceLimit => "resource limit reached",
            Self::CleanupIncomplete => "cleanup incomplete",
            Self::InternalInvariantWarning => "internal invariant violated",
        }
    }

    /// Warning model metadata
    pub const fn all_codes() -> &'static [Self] {
        &[
            Self::DaemonUnavailable,
            Self::ProtocolMismatch,
            Self::PermissionDenied,
            Self::SyscallFailed,
            Self::MmapFailed,
            Self::InvalidConfig,
            Self::MessageTooLarge,
            Self::BackpressurePinnedSlot,
            Self::MessagesSkipped,
            Self::Timeout,
            Self::Disconnected,
            Self::SegmentAbiMismatch,
            Self::SegmentQuarantined,
            Self::SubscriberLimitReached,
            Self::ResourceLimit,
            Self::CleanupIncomplete,
            Self::InternalInvariantWarning,
        ]
    }
}

/// Internal documentation
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Warning {
    component: &'static str,
    operation: &'static str,
    sequence: u64,
    topic_hash: u64,
    stream_id: u64,
    monotonic_timestamp_ns: u64,
    os_errno: i32,
    code: WarningCode,
    context_flags: u8,
}

impl Warning {
    /// Warning model metadata
    pub const fn new(code: WarningCode, component: &'static str, operation: &'static str) -> Self {
        Self {
            component,
            operation,
            sequence: 0,
            topic_hash: 0,
            stream_id: 0,
            monotonic_timestamp_ns: 0,
            os_errno: 0,
            code,
            context_flags: 0,
        }
    }

    /// Warning model metadata
    pub const fn code(&self) -> WarningCode {
        self.code
    }

    /// Returns a value
    pub const fn component(&self) -> &'static str {
        self.component
    }

    /// Returns a value
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Returns a value
    pub const fn message(&self) -> &'static str {
        self.code.description()
    }

    /// Warning model metadata
    pub const fn os_errno(&self) -> Option<i32> {
        if self.context_flags & HAS_OS_ERRNO == 0 {
            None
        } else {
            Some(self.os_errno)
        }
    }

    /// Sequence metadata
    pub const fn sequence(&self) -> Option<Sequence> {
        if self.context_flags & HAS_SEQUENCE == 0 {
            None
        } else {
            Some(Sequence::new(self.sequence))
        }
    }

    /// Topic runtime metadata
    pub const fn topic_hash(&self) -> Option<TopicHash> {
        if self.context_flags & HAS_TOPIC_HASH == 0 {
            None
        } else {
            Some(TopicHash::new(self.topic_hash))
        }
    }

    /// Stream runtime metadata
    pub const fn stream_id(&self) -> Option<StreamId> {
        if self.context_flags & HAS_STREAM_ID == 0 {
            None
        } else {
            Some(StreamId::new(self.stream_id))
        }
    }

    /// Returns a value
    pub const fn monotonic_timestamp_ns(&self) -> Option<u64> {
        if self.context_flags & HAS_MONOTONIC_TIMESTAMP_NS == 0 {
            None
        } else {
            Some(self.monotonic_timestamp_ns)
        }
    }

    /// Warning model metadata
    pub const fn with_os_errno(mut self, os_errno: i32) -> Self {
        self.os_errno = os_errno;
        self.context_flags |= HAS_OS_ERRNO;
        self
    }

    /// Warning model metadata
    pub const fn with_sequence(mut self, sequence: Sequence) -> Self {
        self.sequence = sequence.as_u64();
        self.context_flags |= HAS_SEQUENCE;
        self
    }

    /// Warning model metadata
    pub const fn with_topic_hash(mut self, topic_hash: TopicHash) -> Self {
        self.topic_hash = topic_hash.as_u64();
        self.context_flags |= HAS_TOPIC_HASH;
        self
    }

    /// Warning model metadata
    pub const fn with_stream_id(mut self, stream_id: StreamId) -> Self {
        self.stream_id = stream_id.as_u64();
        self.context_flags |= HAS_STREAM_ID;
        self
    }

    /// Warning model metadata
    pub const fn with_monotonic_timestamp_ns(mut self, monotonic_timestamp_ns: u64) -> Self {
        self.monotonic_timestamp_ns = monotonic_timestamp_ns;
        self.context_flags |= HAS_MONOTONIC_TIMESTAMP_NS;
        self
    }
}
