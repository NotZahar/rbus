use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicU64, Ordering};
use std::alloc::System;

static ALLOCATION_COUNT: AtomicU64 = AtomicU64::new(0);

/// Benchmark metadata
pub struct CountingAllocator;

// SAFETY: Required invariants are upheld
// Internal note
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Required invariants are upheld
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: Required invariants are upheld
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: Required invariants are upheld
        let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
        if !new_pointer.is_null() {
            ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        new_pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Required invariants are upheld
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        pointer
    }
}

/// Returns a value
pub fn allocation_count() -> u64 {
    ALLOCATION_COUNT.load(Ordering::Relaxed)
}

/// Returns a value
pub fn allocation_delta(start_count: u64) -> u64 {
    allocation_count().saturating_sub(start_count)
}
