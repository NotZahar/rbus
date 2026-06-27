use core::mem::{align_of, size_of};

use rbus_core::constants::CACHE_LINE_BYTES;
use rbus_core::layout::{SegmentHeader, SlotHeader, calculate_segment_layout};
use rbus_core::warning::WarningCode;

#[test]
fn segment_and_slot_headers_are_cache_line_aligned() {
    assert_eq!(align_of::<SegmentHeader>(), CACHE_LINE_BYTES);
    assert_eq!(align_of::<SlotHeader>(), CACHE_LINE_BYTES);
    assert_eq!(size_of::<SlotHeader>() % CACHE_LINE_BYTES, 0);
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
