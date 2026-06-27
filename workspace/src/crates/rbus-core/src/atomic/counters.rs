//! Counter helpers
use core::sync::atomic::{AtomicU64, Ordering};

/// Internal documentation
pub fn increment_relaxed_counter(counter: &AtomicU64) -> u64 {
    counter.fetch_add(1, Ordering::Relaxed).saturating_add(1)
}
