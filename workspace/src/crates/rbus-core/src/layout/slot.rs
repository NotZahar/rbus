//! `SlotHeader` stream segment ABI
use core::sync::atomic::AtomicU64;

/// Slot metadata
#[repr(C, align(64))]
pub struct SlotHeader {
    /// Sequence metadata
    pub state_seq: AtomicU64,
    /// Slot metadata
    pub readers_mask: AtomicU64,
    /// Payload metadata
    pub len: u32,
    /// Slot metadata
    pub flags: u32,
    /// Internal documentation
    pub timestamp_ns: u64,
    /// Internal documentation
    pub reserved: [u64; 4],
}

impl SlotHeader {
    /// Creates a value
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
