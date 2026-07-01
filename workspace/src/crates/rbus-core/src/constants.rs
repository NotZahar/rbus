//! Version, ABI and protocol constants shared by all rbus crates

/// Public Rust API version reported by diagnostics
pub const RBUS_API_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Magic value stored at the start of every stream segment
pub const STREAM_SEGMENT_MAGIC: u64 = 0x5242_5553_5348_4d31;

/// Current stream segment ABI version
pub const STREAM_SEGMENT_ABI_VERSION: u32 = 5;

/// Backward-compatible alias for stream segment magic
pub const SHARED_MEMORY_MAGIC: u64 = STREAM_SEGMENT_MAGIC;

/// Backward-compatible alias for stream segment ABI version
pub const SHARED_MEMORY_ABI_VERSION: u32 = STREAM_SEGMENT_ABI_VERSION;

/// Magic value stored at the start of every topic wait segment
pub const TOPIC_WAIT_MAGIC: u64 = 0x5242_5553_5457_5431;

/// Current topic wait segment ABI version
pub const TOPIC_WAIT_ABI_VERSION: u32 = 1;

/// Segment flag set when the daemon has observed publisher shutdown
pub const SEGMENT_FLAG_CLOSED: u64 = 1;

/// Current control-plane frame protocol version
pub const CONTROL_PROTOCOL_VERSION: u16 = 2;

/// Cache-line size assumed by shared-memory layout padding
pub const CACHE_LINE_BYTES: usize = 64;

/// Bit index reserved for the writer claim inside `readers_mask`
pub const SLOT_WRITER_CLAIM_SHIFT: u32 = u64::BITS - 1;

/// Mask bit used by a writer while it owns a slot
pub const SLOT_WRITER_CLAIM_BIT: u64 = 1_u64 << SLOT_WRITER_CLAIM_SHIFT;

/// Maximum number of reader bits available in one stream
pub const MAX_SUBSCRIBERS_PER_STREAM: u8 = SLOT_WRITER_CLAIM_SHIFT as u8;

/// Default maximum payload size accepted by topic configuration
pub const DEFAULT_MAX_MESSAGE_SIZE: usize = 8 * 1024 * 1024;

/// Hard upper bound for one stream segment mapping
pub const MAX_SEGMENT_SIZE: usize = 8 * 1024 * 1024 * 1024;

/// Component name attached to warnings emitted by `rbus-core`
pub const COMPONENT_CORE: &str = "rbus-core";
