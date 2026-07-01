//! Atomic orderings used by the shared-memory ring protocol

use core::sync::atomic::Ordering;

/// Publishes slot metadata after the payload write is complete
pub const WRITER_COMMIT_ORDERING: Ordering = Ordering::Release;

/// Acquires a slot while observing existing reader pins
pub const WRITER_CLAIM_ORDERING: Ordering = Ordering::AcqRel;

/// Observes reader pins when a writer claim compare-exchange fails
pub const WRITER_CLAIM_FAILURE_ORDERING: Ordering = Ordering::Acquire;

/// Releases the writer claim after commit or rollback
pub const WRITER_CLAIM_CLEAR_ORDERING: Ordering = Ordering::Release;

/// Publishes the encoded slot state transition
pub const WRITER_STATE_UPDATE_ORDERING: Ordering = Ordering::Release;

/// Observes committed writer state before reading slot contents
pub const READER_OBSERVE_ORDERING: Ordering = Ordering::Acquire;

/// Observes slot metadata after the committed state is visible
pub const READER_SLOT_OBSERVE_ORDERING: Ordering = Ordering::Acquire;

/// Pins a slot while preserving writer claim ordering
pub const READER_PIN_ORDERING: Ordering = Ordering::AcqRel;

/// Observes writer claim state when reader pinning fails
pub const READER_PIN_FAILURE_ORDERING: Ordering = Ordering::Acquire;

/// Ordering for diagnostic counters that do not synchronize data visibility
pub const COUNTER_ORDERING: Ordering = Ordering::Relaxed;

/// Updates reader masks on release paths
pub const READER_MASK_UPDATE_ORDERING: Ordering = Ordering::AcqRel;

/// Updates stream lifecycle flags shared with the daemon
pub const SEGMENT_FLAG_UPDATE_ORDERING: Ordering = Ordering::AcqRel;

/// Observes stream lifecycle flags shared with the daemon
pub const SEGMENT_FLAG_OBSERVE_ORDERING: Ordering = Ordering::Acquire;
