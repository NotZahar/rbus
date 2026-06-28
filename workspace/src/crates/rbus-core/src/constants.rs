//! Version and ABI metadata

/// Public API metadata
pub const RBUS_API_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Stream segment ABI metadata
pub const STREAM_SEGMENT_MAGIC: u64 = 0x5242_5553_5348_4d31;

/// Stream segment ABI metadata
pub const STREAM_SEGMENT_ABI_VERSION: u32 = 4;

/// Stream segment ABI metadata
pub const SHARED_MEMORY_MAGIC: u64 = STREAM_SEGMENT_MAGIC;

/// Stream segment ABI metadata
pub const SHARED_MEMORY_ABI_VERSION: u32 = STREAM_SEGMENT_ABI_VERSION;

/// Topic wait segment ABI metadata
pub const TOPIC_WAIT_MAGIC: u64 = 0x5242_5553_5457_5431;

/// Topic wait segment ABI metadata
pub const TOPIC_WAIT_ABI_VERSION: u32 = 1;

/// Daemon runtime metadata
pub const SEGMENT_FLAG_CLOSED: u64 = 1;

/// Control protocol metadata
pub const CONTROL_PROTOCOL_VERSION: u16 = 2;

/// Internal documentation
pub const CACHE_LINE_BYTES: usize = 64;

/// Subscriber runtime metadata
pub const SLOT_WRITER_CLAIM_SHIFT: u32 = u64::BITS - 1;

/// Subscriber runtime metadata
pub const SLOT_WRITER_CLAIM_BIT: u64 = 1_u64 << SLOT_WRITER_CLAIM_SHIFT;

/// Subscriber runtime metadata
pub const MAX_SUBSCRIBERS_PER_STREAM: u8 = SLOT_WRITER_CLAIM_SHIFT as u8;

/// Internal documentation
pub const DEFAULT_MAX_MESSAGE_SIZE: usize = 8 * 1024 * 1024;

/// Internal documentation
pub const MAX_SEGMENT_SIZE: usize = 8 * 1024 * 1024 * 1024;

/// Warning model metadata
pub const COMPONENT_CORE: &str = "rbus-core";
