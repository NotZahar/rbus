//! `SegmentHeader` stream segment ABI
use core::sync::atomic::AtomicU64;

use crate::constants::SEGMENT_FLAG_CLOSED;

/// Stream runtime metadata
#[repr(C, align(64))]
pub struct SegmentHeader {
    /// Magic value
    pub magic: u64,
    /// Stream segment ABI metadata
    pub abi_version: u32,
    /// Internal documentation
    pub header_size: u32,
    /// Internal documentation
    pub segment_size: u64,
    /// Stream id
    pub stream_id: u64,
    /// Publisher id
    pub publisher_id: u64,
    /// Topic hash
    pub topic_hash: u64,
    /// Payload metadata
    pub slot_size: u64,
    /// Depth ring buffer
    pub depth: u64,
    /// Sequence metadata
    pub write_sequence: AtomicU64,
    /// Internal documentation
    pub flags: AtomicU64,
    /// Offset counters
    pub counters_offset: u64,
    /// Offset slot headers
    pub slots_offset: u64,
    /// Offset payload area
    pub payload_offset: u64,
    /// Stream segment ABI metadata
    pub reserved: [u64; 19],
}

impl SegmentHeader {
    /// Daemon runtime metadata
    pub fn is_closed(&self, ordering: core::sync::atomic::Ordering) -> bool {
        self.flags.load(ordering) & SEGMENT_FLAG_CLOSED != 0
    }
}
