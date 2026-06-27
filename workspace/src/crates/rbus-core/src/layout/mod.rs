//! Shared-memory layout rbus
pub mod abi;
pub mod align;
pub mod offsets;
pub mod segment;
pub mod slot;

pub use offsets::{SegmentLayout, calculate_segment_layout};
pub use segment::SegmentHeader;
pub use slot::SlotHeader;
