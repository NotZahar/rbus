use core::mem::{align_of, offset_of, size_of};
use core::sync::atomic::AtomicU64;

use rbus_core::constants::{
    CACHE_LINE_BYTES, SHARED_MEMORY_ABI_VERSION, STREAM_SEGMENT_ABI_VERSION,
};
use rbus_core::layout::{SegmentHeader, SlotHeader, calculate_segment_layout};
use rbus_core::warning::WarningCode;

const STREAM_ABI_WITHOUT_WAKE_FIELDS_VERSION: u32 = 4;

#[test]
fn segment_and_slot_headers_are_cache_line_aligned() {
    assert_eq!(align_of::<SegmentHeader>(), CACHE_LINE_BYTES);
    assert_eq!(align_of::<SlotHeader>(), CACHE_LINE_BYTES);
    assert_eq!(size_of::<SlotHeader>() % CACHE_LINE_BYTES, 0);
}

#[test]
fn segment_header_v4_has_no_stream_wake_fields_between_sequence_and_flags() {
    assert_eq!(
        STREAM_SEGMENT_ABI_VERSION,
        STREAM_ABI_WITHOUT_WAKE_FIELDS_VERSION
    );
    assert_eq!(SHARED_MEMORY_ABI_VERSION, STREAM_SEGMENT_ABI_VERSION);
    assert_eq!(
        offset_of!(SegmentHeader, flags),
        offset_of!(SegmentHeader, write_sequence) + size_of::<AtomicU64>()
    );
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
