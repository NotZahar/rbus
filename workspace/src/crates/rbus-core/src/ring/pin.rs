//! Reader and writer bit helpers for slot pinning

use crate::constants::{COMPONENT_CORE, MAX_SUBSCRIBERS_PER_STREAM, SLOT_WRITER_CLAIM_BIT};
use crate::ids::SubscriberId;
use crate::result::RbusResult;
use crate::warning::{Warning, WarningCode};

/// Returns the reader bit reserved for a subscriber id
pub fn reader_bit_mask(subscriber_id: SubscriberId) -> RbusResult<u64> {
    if subscriber_id.as_u8() >= MAX_SUBSCRIBERS_PER_STREAM {
        return Err(Warning::new(
            WarningCode::SubscriberLimitReached,
            COMPONENT_CORE,
            "reader_bit_mask",
        ));
    }
    Ok(1u64 << subscriber_id.as_u8())
}

/// Returns true when any reader bit is pinned
pub const fn has_pinned_readers(readers_mask: u64) -> bool {
    readers_mask & !SLOT_WRITER_CLAIM_BIT != 0
}

/// Returns true when the writer owns the slot claim bit
pub const fn has_writer_claim(readers_mask: u64) -> bool {
    readers_mask & SLOT_WRITER_CLAIM_BIT != 0
}
