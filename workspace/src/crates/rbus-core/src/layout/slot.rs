//! `SlotHeader` stream segment ABI
use core::sync::atomic::AtomicU64;

/// Per-slot header stored before each payload region
#[repr(C, align(64))]
pub struct SlotHeader {
    /// Encoded sequence and writer-in-progress bit for this slot
    pub state_seq: AtomicU64,
    /// Reader pins plus the writer claim bit
    pub readers_mask: AtomicU64,
    /// Number of valid payload bytes in this slot
    pub len: u32,
    /// Reserved slot flags
    pub flags: u32,
    /// Publisher-provided monotonic timestamp in nanoseconds
    pub timestamp_ns: u64,
    /// Reserved ABI space that must remain zeroed
    pub reserved: [u64; 4],
}

impl SlotHeader {
    /// Creates an empty slot header
    pub fn new() -> Self {
        Self {
            state_seq: AtomicU64::new(0),
            readers_mask: AtomicU64::new(0),
            len: 0,
            flags: 0,
            timestamp_ns: 0,
            reserved: [0; 4],
        }
    }
}

impl Default for SlotHeader {
    fn default() -> Self {
        Self::new()
    }
}
