//! Internal documentation

use core::sync::atomic::Ordering;

/// Internal documentation
pub const WRITER_COMMIT_ORDERING: Ordering = Ordering::Release;

/// Internal documentation
pub const WRITER_CLAIM_ORDERING: Ordering = Ordering::AcqRel;

/// Internal documentation
pub const WRITER_CLAIM_FAILURE_ORDERING: Ordering = Ordering::Acquire;

/// Internal documentation
pub const WRITER_CLAIM_CLEAR_ORDERING: Ordering = Ordering::Release;

/// Internal documentation
pub const WRITER_STATE_UPDATE_ORDERING: Ordering = Ordering::Release;

/// Internal documentation
pub const READER_OBSERVE_ORDERING: Ordering = Ordering::Acquire;

/// Internal documentation
pub const READER_SLOT_OBSERVE_ORDERING: Ordering = Ordering::Acquire;

/// Internal documentation
pub const READER_PIN_ORDERING: Ordering = Ordering::AcqRel;

/// Internal documentation
pub const READER_PIN_FAILURE_ORDERING: Ordering = Ordering::Acquire;

/// Payload metadata
pub const COUNTER_ORDERING: Ordering = Ordering::Relaxed;

/// Internal documentation
pub const READER_MASK_UPDATE_ORDERING: Ordering = Ordering::AcqRel;

/// Daemon runtime metadata
pub const SEGMENT_FLAG_UPDATE_ORDERING: Ordering = Ordering::AcqRel;

/// Internal documentation
pub const SEGMENT_FLAG_OBSERVE_ORDERING: Ordering = Ordering::Acquire;
