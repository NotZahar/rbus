//! `SegmentHeader` stream segment ABI
use core::sync::atomic::AtomicU64;

use crate::constants::SEGMENT_FLAG_CLOSED;

/// Fixed header placed at the beginning of every stream segment
#[repr(C, align(64))]
pub struct SegmentHeader {
    /// Magic value
    pub magic: u64,
    /// ABI version used to interpret the segment layout
    pub abi_version: u32,
    /// Size of this header after ABI-required alignment
    pub header_size: u32,
    /// Total byte length of the mapped segment
    pub segment_size: u64,
    /// Stream id
    pub stream_id: u64,
    /// Publisher id
    pub publisher_id: u64,
    /// Topic hash
    pub topic_hash: u64,
    /// Maximum payload bytes stored in each slot
    pub slot_size: u64,
    /// Depth ring buffer
    pub depth: u64,
    /// Next sequence number reserved by the publisher
    pub write_sequence: AtomicU64,
    /// Segment lifecycle flags shared with the daemon
    pub flags: AtomicU64,
    /// Offset counters
    pub counters_offset: u64,
    /// Offset slot headers
    pub slots_offset: u64,
    /// Offset payload area
    pub payload_offset: u64,
    /// Reserved ABI space that must remain zeroed
    pub reserved: [u64; 19],
}

impl SegmentHeader {
    /// Returns whether the daemon marked this publisher stream as closed
    pub fn is_closed(&self, ordering: core::sync::atomic::Ordering) -> bool {
        self.flags.load(ordering) & SEGMENT_FLAG_CLOSED != 0
    }
}
