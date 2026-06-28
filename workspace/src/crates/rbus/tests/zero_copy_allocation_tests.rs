use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rbus::{Bus, TopicConfig};

mod common;

const TOPIC: &str = "/zero/copy/allocations";
const MAX_MESSAGE_SIZE: usize = 64;
const STREAM_DEPTH: usize = 8;
const PAYLOAD: &[u8] = b"rbus";
const ZERO_ALLOCATIONS: u64 = 0;
const ALLOCATION_INCREMENT: u64 = 1;

static ALLOCATION_COUNTING_ENABLED: AtomicBool = AtomicBool::new(false);
static ALLOCATION_COUNT: AtomicU64 = AtomicU64::new(ZERO_ALLOCATIONS);

struct AllocationCountingAllocator;

#[global_allocator]
static GLOBAL_ALLOCATOR: AllocationCountingAllocator = AllocationCountingAllocator;

unsafe impl GlobalAlloc for AllocationCountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: instrumentation does not change `System` allocator preconditions
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: instrumentation does not change `System` allocator preconditions
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count_allocation();
        // SAFETY: instrumentation does not change `System` allocator preconditions
        unsafe { System.realloc(pointer, layout, new_size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: instrumentation does not change `System` allocator preconditions
        unsafe { System.dealloc(pointer, layout) };
    }
}

#[test]
fn zero_copy_hot_path_does_not_allocate() {
    let daemon = common::DaemonProcess::start("zero-copy-allocations");
    let bus = Bus::connect(&daemon.domain).expect("domain should connect");
    let mut publisher = bus
        .publisher(TOPIC, TopicConfig::new(MAX_MESSAGE_SIZE, STREAM_DEPTH))
        .expect("publisher operation should succeed");
    let subscriber = bus
        .subscriber(TOPIC)
        .expect("subscriber operation should succeed");

    let (publish_result, publish_allocation_count) = measure_allocations(|| {
        let mut message = match publisher.loan(PAYLOAD.len()) {
            Ok(message) => message,
            Err(warning) => return Err(warning),
        };
        message.as_mut_slice().copy_from_slice(PAYLOAD);
        message.publish()
    });
    let sequence = publish_result.expect("publish should succeed");
    assert_eq!(publish_allocation_count, ZERO_ALLOCATIONS);

    let (receive_result, receive_allocation_count) = measure_allocations(|| subscriber.try_recv());
    let received_message = receive_result
        .expect("message should be available")
        .expect("message should be available");
    assert_eq!(receive_allocation_count, ZERO_ALLOCATIONS);
    assert_eq!(received_message.sequence(), sequence);
    assert_eq!(received_message.as_slice(), PAYLOAD);
    drop(received_message);

    let (empty_receive_result, empty_receive_allocation_count) =
        measure_allocations(|| subscriber.try_recv());
    assert!(
        empty_receive_result
            .expect("message should be available")
            .is_none()
    );
    assert_eq!(empty_receive_allocation_count, ZERO_ALLOCATIONS);
}

fn measure_allocations<OperationResult>(
    operation: impl FnOnce() -> OperationResult,
) -> (OperationResult, u64) {
    ALLOCATION_COUNT.store(ZERO_ALLOCATIONS, Ordering::SeqCst);
    ALLOCATION_COUNTING_ENABLED.store(true, Ordering::SeqCst);
    let guard = AllocationCountingGuard;
    let result = operation();
    drop(guard);
    (result, ALLOCATION_COUNT.load(Ordering::SeqCst))
}

fn count_allocation() {
    if ALLOCATION_COUNTING_ENABLED.load(Ordering::Relaxed) {
        ALLOCATION_COUNT.fetch_add(ALLOCATION_INCREMENT, Ordering::Relaxed);
    }
}

struct AllocationCountingGuard;

impl Drop for AllocationCountingGuard {
    fn drop(&mut self) {
        ALLOCATION_COUNTING_ENABLED.store(false, Ordering::SeqCst);
    }
}
