//! Stream segment layout calculation

use core::mem::{align_of, size_of};

use crate::cache::align_value_to;
use crate::constants::{CACHE_LINE_BYTES, COMPONENT_CORE, MAX_SEGMENT_SIZE};
use crate::layout::segment::SegmentHeader;
use crate::layout::slot::SlotHeader;
use crate::result::RbusResult;
use crate::stats::StreamCounters;
use crate::warning::{Warning, WarningCode};

/// Byte offsets and strides for all regions inside a stream segment
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SegmentLayout {
    /// Total byte length of the segment
    pub segment_size: usize,
    /// Aligned byte length of the segment header
    pub header_size: usize,
    /// Offset counters
    pub counters_offset: usize,
    /// Offset slot headers
    pub slots_offset: usize,
    /// Offset payload area
    pub payload_offset: usize,
    /// Aligned distance between consecutive payload slots
    pub payload_stride: usize,
    /// Usable payload bytes in each slot
    pub slot_size: usize,
    /// Depth ring buffer
    pub depth: usize,
}

/// Calculates a stream segment layout for a slot size and power-of-two depth
pub fn calculate_segment_layout(slot_size: usize, depth: usize) -> RbusResult<SegmentLayout> {
    if slot_size == 0 || depth == 0 || !depth.is_power_of_two() {
        return Err(Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_CORE,
            "calculate_segment_layout",
        ));
    }

    let header_size =
        align_value_to(size_of::<SegmentHeader>(), CACHE_LINE_BYTES).ok_or_else(|| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_CORE,
                "calculate_segment_layout",
            )
        })?;
    let counters_offset = header_size;
    let counters_end = counters_offset
        .checked_add(size_of::<StreamCounters>())
        .and_then(|value| align_value_to(value, CACHE_LINE_BYTES))
        .ok_or_else(|| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_CORE,
                "calculate_segment_layout",
            )
        })?;

    let slots_offset = counters_end;
    let slot_headers_size = size_of::<SlotHeader>().checked_mul(depth).ok_or_else(|| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_CORE,
            "calculate_segment_layout",
        )
    })?;
    let payload_offset = slots_offset
        .checked_add(slot_headers_size)
        .and_then(|value| align_value_to(value, CACHE_LINE_BYTES))
        .ok_or_else(|| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_CORE,
                "calculate_segment_layout",
            )
        })?;

    let payload_stride = align_value_to(slot_size, CACHE_LINE_BYTES).ok_or_else(|| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_CORE,
            "calculate_segment_layout",
        )
    })?;
    let payload_size = payload_stride.checked_mul(depth).ok_or_else(|| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_CORE,
            "calculate_segment_layout",
        )
    })?;
    let segment_size = payload_offset.checked_add(payload_size).ok_or_else(|| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_CORE,
            "calculate_segment_layout",
        )
    })?;

    if segment_size > MAX_SEGMENT_SIZE
        || align_of::<SegmentHeader>() > CACHE_LINE_BYTES
        || align_of::<SlotHeader>() > CACHE_LINE_BYTES
    {
        return Err(Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_CORE,
            "calculate_segment_layout",
        ));
    }

    Ok(SegmentLayout {
        segment_size,
        header_size,
        counters_offset,
        slots_offset,
        payload_offset,
        payload_stride,
        slot_size,
        depth,
    })
}
