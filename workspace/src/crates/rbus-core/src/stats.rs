//! Internal documentation

use core::mem::size_of;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::constants::CACHE_LINE_BYTES;

const COUNTER_INITIAL_VALUE: u64 = 0;
const STREAM_COUNTER_RESERVED_LINES: usize = 9;
const COUNTER_PADDING_U64S: usize = (CACHE_LINE_BYTES - size_of::<AtomicU64>()) / size_of::<u64>();

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

/// Stream counter stored on its own cache line
#[repr(C, align(64))]
pub struct CacheLineCounter {
    value: AtomicU64,
    _padding: [u64; COUNTER_PADDING_U64S],
}

impl CacheLineCounter {
    /// Creates a value
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

/// Internal documentation
#[repr(C, align(64))]
pub struct StreamCounters {
    /// Internal documentation
    pub published_messages: CacheLineCounter,
    /// Internal documentation
    pub received_messages: CacheLineCounter,
    /// Internal documentation
    pub skipped_messages: CacheLineCounter,
    /// Slot metadata
    pub backpressure_pinned_slot: CacheLineCounter,
    /// Warning model metadata
    pub warnings_total: CacheLineCounter,
    /// Internal documentation
    pub attach_count: CacheLineCounter,
    /// Stream runtime metadata
    pub disconnected_streams: CacheLineCounter,
    /// Shared memory ABI metadata
    pub reserved: [CacheLineCounter; STREAM_COUNTER_RESERVED_LINES],
}

impl StreamCounters {
    /// Creates a value
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
