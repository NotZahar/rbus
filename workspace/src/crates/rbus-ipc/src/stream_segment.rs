//! Creation, mapping and inspection of stream shared-memory segments

use rbus_core::atomic::ordering::{
    COUNTER_ORDERING, READER_MASK_UPDATE_ORDERING, SEGMENT_FLAG_OBSERVE_ORDERING,
    SEGMENT_FLAG_UPDATE_ORDERING,
};
use rbus_core::constants::{SEGMENT_FLAG_CLOSED, STREAM_SEGMENT_ABI_VERSION, STREAM_SEGMENT_MAGIC};
use rbus_core::ids::{PublisherId, StreamId, SubscriberId, TopicHash};
use rbus_core::layout::{SegmentHeader, SegmentLayout, SlotHeader, calculate_segment_layout};
use rbus_core::ring::pin::reader_bit_mask;
use rbus_core::stats::{Stats, StreamCounters};
use rbus_core::warning::{Warning, WarningCode};

use crate::fd::OwnedFileDescriptor;
use crate::memfd::{create_memfd, seal_file_descriptor_size};
use crate::mmap::MappedRegion;

const COMPONENT_IPC: &str = "rbus-ipc";
const COUNTER_INCREMENT: u64 = 1;
const SEGMENT_HEADER_OFFSET: usize = 0;

/// Static metadata required to attach an existing stream segment
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamSegmentMetadata {
    /// Stream id daemon registry
    pub stream_id: StreamId,
    /// Publisher id daemon registry
    pub publisher_id: PublisherId,
    /// Stable topic hash
    pub topic_hash: TopicHash,
    /// Total byte length of the mapped segment
    pub segment_size: usize,
    /// Maximum payload bytes stored in each slot
    pub slot_size: usize,
    /// Depth ring buffer
    pub depth: usize,
}

/// Newly created stream segment and its owned descriptor
pub struct CreatedStreamSegment {
    /// Metadata needed by subscribers to map the segment
    pub metadata: StreamSegmentMetadata,
    /// Memfd backing the stream segment
    pub file_descriptor: OwnedFileDescriptor,
}

/// Point-in-time stream segment header and counter state
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamSegmentSnapshot {
    /// Metadata used to attach the stream
    pub metadata: StreamSegmentMetadata,
    /// ABI version read from the mapped segment header
    pub abi_version: u32,
    /// Segment lifecycle flags read from the header
    pub flags: u64,
    /// Publisher write sequence read from the header
    pub write_sequence: u64,
    /// Snapshot stream counters
    pub stats: Stats,
}

/// Creates and initializes a writable stream segment
pub fn create_stream_segment(
    publisher_id: PublisherId,
    stream_id: StreamId,
    topic_hash: TopicHash,
    slot_size: usize,
    depth: usize,
) -> Result<CreatedStreamSegment, Warning> {
    let layout = calculate_segment_layout(slot_size, depth)?;
    let metadata = StreamSegmentMetadata {
        stream_id,
        publisher_id,
        topic_hash,
        segment_size: layout.segment_size,
        slot_size,
        depth,
    };
    let file_descriptor =
        create_memfd(&memfd_name(publisher_id, stream_id), metadata.segment_size)?;
    seal_file_descriptor_size(&file_descriptor)?;
    let mapping = MappedRegion::map_shared(
        file_descriptor.as_raw_file_descriptor(),
        metadata.segment_size,
        true,
    )?;
    initialize_segment(&mapping, metadata, layout)?;
    Ok(CreatedStreamSegment {
        metadata,
        file_descriptor,
    })
}

/// Maps and validates an existing stream segment
pub fn map_stream_segment(
    file_descriptor: &OwnedFileDescriptor,
    metadata: StreamSegmentMetadata,
    writable: bool,
) -> Result<(MappedRegion, SegmentLayout), Warning> {
    let layout = calculate_segment_layout(metadata.slot_size, metadata.depth)?;
    if layout.segment_size != metadata.segment_size {
        return Err(Warning::new(
            WarningCode::SegmentAbiMismatch,
            COMPONENT_IPC,
            "map_stream_segment::segment_size",
        )
        .with_stream_id(metadata.stream_id));
    }
    let mapping = MappedRegion::map_shared(
        file_descriptor.as_raw_file_descriptor(),
        metadata.segment_size,
        writable,
    )?;
    validate_segment_header(&mapping, metadata, layout)?;
    Ok((mapping, layout))
}

/// Reads a diagnostic snapshot from a stream segment
pub fn snapshot_stream_segment(
    file_descriptor: &OwnedFileDescriptor,
    metadata: StreamSegmentMetadata,
    subscriber_count: u64,
) -> Result<StreamSegmentSnapshot, Warning> {
    let (mapping, layout) = map_stream_segment(file_descriptor, metadata, false)?;
    let header = segment_header(&mapping)?;
    let flags = header.flags.load(SEGMENT_FLAG_OBSERVE_ORDERING);
    let write_sequence = header.write_sequence.load(COUNTER_ORDERING);
    let publisher_alive = flags & SEGMENT_FLAG_CLOSED == 0;
    Ok(StreamSegmentSnapshot {
        metadata,
        abi_version: header.abi_version,
        flags,
        write_sequence,
        stats: stream_counters(&mapping, layout)?.snapshot(
            write_sequence,
            subscriber_count,
            publisher_alive,
        ),
    })
}

/// Marks a stream as closed and increments its disconnect counter
pub fn mark_stream_closed(
    file_descriptor: &OwnedFileDescriptor,
    metadata: StreamSegmentMetadata,
) -> Result<(), Warning> {
    let (mapping, layout) = map_stream_segment(file_descriptor, metadata, true)?;
    segment_header(&mapping)?
        .flags
        .fetch_or(SEGMENT_FLAG_CLOSED, SEGMENT_FLAG_UPDATE_ORDERING);
    stream_counters(&mapping, layout)?
        .disconnected_streams
        .fetch_add(COUNTER_INCREMENT, COUNTER_ORDERING);
    Ok(())
}

/// Clears all reader pins owned by one subscriber id
pub fn clear_subscriber_reader_bits(
    file_descriptor: &OwnedFileDescriptor,
    metadata: StreamSegmentMetadata,
    subscriber_id: SubscriberId,
) -> Result<u64, Warning> {
    let reader_bit = reader_bit_mask(subscriber_id)?;
    let (mapping, layout) = map_stream_segment(file_descriptor, metadata, true)?;
    let mut cleared_count = 0_u64;
    for slot_index in 0..layout.depth {
        let previous_mask = slot_header(&mapping, layout, slot_index)?
            .readers_mask
            .fetch_and(!reader_bit, READER_MASK_UPDATE_ORDERING);
        if previous_mask & reader_bit != 0 {
            cleared_count = cleared_count
                .checked_add(COUNTER_INCREMENT)
                .ok_or_else(|| {
                    Warning::new(
                        WarningCode::ResourceLimit,
                        COMPONENT_IPC,
                        "clear_subscriber_reader_bits::cleared_count",
                    )
                })?;
        }
    }
    Ok(cleared_count)
}

fn initialize_segment(
    mapping: &MappedRegion,
    metadata: StreamSegmentMetadata,
    layout: SegmentLayout,
) -> Result<(), Warning> {
    let header_size = u32::try_from(layout.header_size).map_err(|_| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_IPC,
            "initialize_segment::header_size",
        )
    })?;

    let header_pointer = mapping
        .typed_pointer::<SegmentHeader>(SEGMENT_HEADER_OFFSET, "initialize_segment::header")?;
    let counters_pointer = mapping
        .typed_pointer::<StreamCounters>(layout.counters_offset, "initialize_segment::counters")?;

    // SAFETY: Pointers were bounds and alignment checked against the newly created mapping
    unsafe {
        core::ptr::write(
            header_pointer.as_ptr(),
            SegmentHeader {
                magic: STREAM_SEGMENT_MAGIC,
                abi_version: STREAM_SEGMENT_ABI_VERSION,
                header_size,
                segment_size: layout.segment_size as u64,
                stream_id: metadata.stream_id.as_u64(),
                publisher_id: metadata.publisher_id.as_u64(),
                topic_hash: metadata.topic_hash.as_u64(),
                slot_size: layout.slot_size as u64,
                depth: layout.depth as u64,
                write_sequence: core::sync::atomic::AtomicU64::new(0),
                flags: core::sync::atomic::AtomicU64::new(0),
                counters_offset: layout.counters_offset as u64,
                slots_offset: layout.slots_offset as u64,
                payload_offset: layout.payload_offset as u64,
                reserved: [0; 19],
            },
        );
        core::ptr::write(counters_pointer.as_ptr(), StreamCounters::new());
    }

    for slot_index in 0..layout.depth {
        let slot_pointer = mapping.typed_pointer::<SlotHeader>(
            slot_offset(layout, slot_index, "initialize_segment::slot_offset")?,
            "initialize_segment::slot",
        )?;
        // SAFETY: The slot pointer was checked for bounds and alignment before writing
        unsafe {
            core::ptr::write(slot_pointer.as_ptr(), SlotHeader::new());
        }
    }

    Ok(())
}

fn validate_segment_header(
    mapping: &MappedRegion,
    metadata: StreamSegmentMetadata,
    layout: SegmentLayout,
) -> Result<(), Warning> {
    let header = segment_header(mapping)?;
    let valid = header.magic == STREAM_SEGMENT_MAGIC
        && header.abi_version == STREAM_SEGMENT_ABI_VERSION
        && header.segment_size == metadata.segment_size as u64
        && header.stream_id == metadata.stream_id.as_u64()
        && header.publisher_id == metadata.publisher_id.as_u64()
        && header.topic_hash == metadata.topic_hash.as_u64()
        && header.slot_size == metadata.slot_size as u64
        && header.depth == metadata.depth as u64
        && header.counters_offset == layout.counters_offset as u64
        && header.slots_offset == layout.slots_offset as u64
        && header.payload_offset == layout.payload_offset as u64;
    if valid {
        Ok(())
    } else {
        Err(Warning::new(
            WarningCode::SegmentAbiMismatch,
            COMPONENT_IPC,
            "validate_segment_header",
        )
        .with_stream_id(metadata.stream_id))
    }
}

fn segment_header(mapping: &MappedRegion) -> Result<&SegmentHeader, Warning> {
    let pointer =
        mapping.typed_pointer::<SegmentHeader>(SEGMENT_HEADER_OFFSET, "segment_header")?;
    // SAFETY: The pointer is within the live mapping and aligned for SegmentHeader
    Ok(unsafe { pointer.as_ref() })
}

fn stream_counters(
    mapping: &MappedRegion,
    layout: SegmentLayout,
) -> Result<&StreamCounters, Warning> {
    let pointer =
        mapping.typed_pointer::<StreamCounters>(layout.counters_offset, "stream_counters")?;
    // SAFETY: The pointer is within a validated stream mapping and aligned for StreamCounters
    Ok(unsafe { pointer.as_ref() })
}

fn slot_header(
    mapping: &MappedRegion,
    layout: SegmentLayout,
    slot_index: usize,
) -> Result<&SlotHeader, Warning> {
    let pointer = mapping.typed_pointer::<SlotHeader>(
        slot_offset(layout, slot_index, "slot_header::offset")?,
        "slot_header",
    )?;
    // SAFETY: The pointer is within a validated stream mapping and aligned for SlotHeader
    Ok(unsafe { pointer.as_ref() })
}

fn slot_offset(
    layout: SegmentLayout,
    slot_index: usize,
    operation: &'static str,
) -> Result<usize, Warning> {
    let slot_headers_offset = slot_index
        .checked_mul(core::mem::size_of::<SlotHeader>())
        .and_then(|offset| layout.slots_offset.checked_add(offset))
        .ok_or_else(|| Warning::new(WarningCode::ResourceLimit, COMPONENT_IPC, operation))?;
    Ok(slot_headers_offset)
}

fn memfd_name(publisher_id: PublisherId, stream_id: StreamId) -> String {
    format!("rbus-p{}-s{}", publisher_id.as_u64(), stream_id.as_u64())
}
