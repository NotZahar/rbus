use core::mem::{align_of, offset_of, size_of};
use core::sync::atomic::AtomicU64;

use rbus_core::constants::{
    CACHE_LINE_BYTES, SHARED_MEMORY_ABI_VERSION, STREAM_SEGMENT_ABI_VERSION,
};
use rbus_core::layout::{SegmentHeader, SlotHeader, calculate_segment_layout};
use rbus_core::stats::{CacheLineCounter, StreamCounters};
use rbus_core::warning::WarningCode;

const STREAM_ABI_CACHE_ALIGNED_COUNTERS_VERSION: u32 = 5;
const CACHE_LINE_START_OFFSET: usize = 0;
const FIRST_COUNTER_INDEX: usize = 0;
const NEXT_COUNTER_INDEX_OFFSET: usize = 1;

#[test]
fn segment_and_slot_headers_are_cache_line_aligned() {
    assert_eq!(align_of::<SegmentHeader>(), CACHE_LINE_BYTES);
    assert_eq!(align_of::<SlotHeader>(), CACHE_LINE_BYTES);
    assert_eq!(size_of::<SlotHeader>() % CACHE_LINE_BYTES, 0);
}

#[test]
fn stream_segment_abi_has_cache_line_aligned_counters_without_header_wake_fields() {
    assert_eq!(
        STREAM_SEGMENT_ABI_VERSION,
        STREAM_ABI_CACHE_ALIGNED_COUNTERS_VERSION
    );
    assert_eq!(SHARED_MEMORY_ABI_VERSION, STREAM_SEGMENT_ABI_VERSION);
    assert_eq!(
        offset_of!(SegmentHeader, flags),
        offset_of!(SegmentHeader, write_sequence) + size_of::<AtomicU64>()
    );
}

#[test]
fn stream_counters_do_not_share_cache_lines_between_write_heavy_fields() {
    assert_eq!(align_of::<CacheLineCounter>(), CACHE_LINE_BYTES);
    assert_eq!(size_of::<CacheLineCounter>(), CACHE_LINE_BYTES);
    let counter_offsets = [
        offset_of!(StreamCounters, published_messages),
        offset_of!(StreamCounters, received_messages),
        offset_of!(StreamCounters, skipped_messages),
        offset_of!(StreamCounters, backpressure_pinned_slot),
        offset_of!(StreamCounters, warnings_total),
        offset_of!(StreamCounters, attach_count),
        offset_of!(StreamCounters, disconnected_streams),
    ];

    for offset in counter_offsets.iter().copied() {
        assert_eq!(offset % CACHE_LINE_BYTES, CACHE_LINE_START_OFFSET);
    }

    for first_index in FIRST_COUNTER_INDEX..counter_offsets.len() {
        for second_index in first_index + NEXT_COUNTER_INDEX_OFFSET..counter_offsets.len() {
            assert_ne!(
                counter_offsets[first_index] / CACHE_LINE_BYTES,
                counter_offsets[second_index] / CACHE_LINE_BYTES
            );
        }
    }
}

#[test]
fn layout_calculation_rejects_non_power_of_two_depth() {
    let warning = calculate_segment_layout(1024, 63).err();
    assert!(matches!(
        warning.map(|value| value.code()),
        Some(WarningCode::InvalidConfig)
    ));
}

#[test]
fn layout_calculation_aligns_offsets() {
    let layout = calculate_segment_layout(1000, 64).expect("layout should be calculated");
    assert_eq!(layout.counters_offset % CACHE_LINE_BYTES, 0);
    assert_eq!(layout.slots_offset % CACHE_LINE_BYTES, 0);
    assert_eq!(layout.payload_offset % CACHE_LINE_BYTES, 0);
    assert_eq!(layout.payload_stride % CACHE_LINE_BYTES, 0);
}
