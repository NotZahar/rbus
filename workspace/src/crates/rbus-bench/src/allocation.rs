use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicU64, Ordering};
use std::alloc::System;

static ALLOCATION_COUNT: AtomicU64 = AtomicU64::new(0);

/// Global allocator wrapper used to count heap allocations
pub struct CountingAllocator;

// SAFETY: The implementation forwards every allocation operation to `System` with the exact
// layout and pointer arguments provided by the allocator contract
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The caller upholds `GlobalAlloc::alloc` preconditions for `layout`
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: The caller passes a pointer and layout previously returned by this allocator
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: The caller upholds `GlobalAlloc::realloc` preconditions for this allocation
        let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
        if !new_pointer.is_null() {
            ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        new_pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The caller upholds `GlobalAlloc::alloc_zeroed` preconditions for `layout`
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        pointer
    }
}

/// Returns the total number of successful allocation calls
pub fn allocation_count() -> u64 {
    ALLOCATION_COUNT.load(Ordering::Relaxed)
}

/// Returns allocations observed since `start_count`
pub fn allocation_delta(start_count: u64) -> u64 {
    allocation_count().saturating_sub(start_count)
}
