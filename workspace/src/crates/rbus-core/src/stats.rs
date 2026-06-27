//! Internal documentation

use core::sync::atomic::{AtomicU64, Ordering};

/// CLI metadata
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Stats {
    /// Internal documentation
    pub published_messages: u64,
    /// Internal documentation
    pub received_messages: u64,
    /// Internal documentation
    pub skipped_messages: u64,
    /// Slot metadata
    pub backpressure_pinned_slot: u64,
    /// Warning model metadata
    pub warnings_total: u64,
    /// Stream runtime metadata
    pub connected_streams: u64,
    /// Stream runtime metadata
    pub disconnected_streams: u64,
    /// Sequence metadata
    pub current_sequence: u64,
    /// Subscriber runtime metadata
    pub subscriber_count: u64,
    /// Publisher runtime metadata
    pub publisher_alive: bool,
    /// Internal documentation
    pub attach_count: u64,
}

/// Internal documentation
#[repr(C, align(64))]
pub struct StreamCounters {
    /// Internal documentation
    pub published_messages: AtomicU64,
    /// Internal documentation
    pub received_messages: AtomicU64,
    /// Internal documentation
    pub skipped_messages: AtomicU64,
    /// Slot metadata
    pub backpressure_pinned_slot: AtomicU64,
    /// Warning model metadata
    pub warnings_total: AtomicU64,
    /// Internal documentation
    pub attach_count: AtomicU64,
    /// Stream runtime metadata
    pub disconnected_streams: AtomicU64,
    /// Shared memory ABI metadata
    pub reserved: [AtomicU64; 9],
}

impl StreamCounters {
    /// Creates a value
    pub fn new() -> Self {
        Self {
            published_messages: AtomicU64::new(0),
            received_messages: AtomicU64::new(0),
            skipped_messages: AtomicU64::new(0),
            backpressure_pinned_slot: AtomicU64::new(0),
            warnings_total: AtomicU64::new(0),
            attach_count: AtomicU64::new(0),
            disconnected_streams: AtomicU64::new(0),
            reserved: core::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    /// Internal documentation
    pub fn snapshot(
        &self,
        current_sequence: u64,
        subscriber_count: u64,
        publisher_alive: bool,
    ) -> Stats {
        Stats {
            published_messages: self.published_messages.load(Ordering::Relaxed),
            received_messages: self.received_messages.load(Ordering::Relaxed),
            skipped_messages: self.skipped_messages.load(Ordering::Relaxed),
            backpressure_pinned_slot: self.backpressure_pinned_slot.load(Ordering::Relaxed),
            warnings_total: self.warnings_total.load(Ordering::Relaxed),
            connected_streams: 1,
            disconnected_streams: self.disconnected_streams.load(Ordering::Relaxed),
            current_sequence,
            subscriber_count,
            publisher_alive,
            attach_count: self.attach_count.load(Ordering::Relaxed),
        }
    }

    /// Warning model metadata
    pub fn increment_warnings_total(&self) {
        self.warnings_total.fetch_add(1, Ordering::Relaxed);
    }
}

impl Default for StreamCounters {
    fn default() -> Self {
        Self::new()
    }
}
