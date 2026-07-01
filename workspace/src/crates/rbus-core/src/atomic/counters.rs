//! Counter helpers
use core::sync::atomic::{AtomicU64, Ordering};

/// Increments a relaxed atomic counter and returns the incremented value
pub fn increment_relaxed_counter(counter: &AtomicU64) -> u64 {
    counter.fetch_add(1, Ordering::Relaxed).saturating_add(1)
}
