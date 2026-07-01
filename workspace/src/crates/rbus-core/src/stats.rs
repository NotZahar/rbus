//! Data-plane counters and diagnostic snapshots

use core::mem::size_of;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::constants::CACHE_LINE_BYTES;

const COUNTER_INITIAL_VALUE: u64 = 0;
const STREAM_COUNTER_RESERVED_LINES: usize = 9;
const COUNTER_PADDING_U64S: usize = (CACHE_LINE_BYTES - size_of::<AtomicU64>()) / size_of::<u64>();

/// Snapshot of stream counters returned by diagnostics
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Stats {
    /// Messages committed by the publisher
    pub published_messages: u64,
    /// Messages delivered to subscribers
    pub received_messages: u64,
    /// Messages skipped because a subscriber cursor fell behind
    pub skipped_messages: u64,
    /// Times a writer found a slot pinned by readers
    pub backpressure_pinned_slot: u64,
    /// Total warnings observed for the stream
    pub warnings_total: u64,
    /// Number of streams currently considered connected in this snapshot
    pub connected_streams: u64,
    /// Number of stream disconnect events observed
    pub disconnected_streams: u64,
    /// Current publisher sequence at snapshot time
    pub current_sequence: u64,
    /// Subscribers attached to the stream
    pub subscriber_count: u64,
    /// Whether the publisher that owns this stream is alive
    pub publisher_alive: bool,
    /// Number of successful subscriber attach operations
    pub attach_count: u64,
}

/// Stream counter stored on its own cache line
#[repr(C, align(64))]
pub struct CacheLineCounter {
    value: AtomicU64,
    _padding: [u64; COUNTER_PADDING_U64S],
}

impl CacheLineCounter {
    /// Creates a cache-line counter with an initial value
    pub const fn new(value: u64) -> Self {
        Self {
            value: AtomicU64::new(value),
            _padding: [COUNTER_INITIAL_VALUE; COUNTER_PADDING_U64S],
        }
    }

    /// Loads a value
    pub fn load(&self, ordering: Ordering) -> u64 {
        self.value.load(ordering)
    }

    /// Stores a value
    pub fn store(&self, value: u64, ordering: Ordering) {
        self.value.store(value, ordering);
    }

    /// Adds a value
    pub fn fetch_add(&self, value: u64, ordering: Ordering) -> u64 {
        self.value.fetch_add(value, ordering)
    }
}

impl Default for CacheLineCounter {
    fn default() -> Self {
        Self::new(COUNTER_INITIAL_VALUE)
    }
}

/// Shared-memory counters for one stream
#[repr(C, align(64))]
pub struct StreamCounters {
    /// Messages committed by the publisher
    pub published_messages: CacheLineCounter,
    /// Messages delivered to subscribers
    pub received_messages: CacheLineCounter,
    /// Messages skipped because a subscriber cursor fell behind
    pub skipped_messages: CacheLineCounter,
    /// Times a writer found a slot pinned by readers
    pub backpressure_pinned_slot: CacheLineCounter,
    /// Total warnings observed for the stream
    pub warnings_total: CacheLineCounter,
    /// Number of successful subscriber attach operations
    pub attach_count: CacheLineCounter,
    /// Number of stream disconnect events observed
    pub disconnected_streams: CacheLineCounter,
    /// Reserved cache-line counters for ABI-compatible expansion
    pub reserved: [CacheLineCounter; STREAM_COUNTER_RESERVED_LINES],
}

impl StreamCounters {
    /// Creates zero-initialized stream counters
    pub fn new() -> Self {
        Self {
            published_messages: CacheLineCounter::default(),
            received_messages: CacheLineCounter::default(),
            skipped_messages: CacheLineCounter::default(),
            backpressure_pinned_slot: CacheLineCounter::default(),
            warnings_total: CacheLineCounter::default(),
            attach_count: CacheLineCounter::default(),
            disconnected_streams: CacheLineCounter::default(),
            reserved: core::array::from_fn(|_| CacheLineCounter::default()),
        }
    }

    /// Loads all counters into a diagnostic snapshot
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

    /// Increments the total warning counter
    pub fn increment_warnings_total(&self) {
        self.warnings_total.fetch_add(1, Ordering::Relaxed);
    }
}

impl Default for StreamCounters {
    fn default() -> Self {
        Self::new()
    }
}
