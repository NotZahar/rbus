//! Topic-level futex wait segment
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use rbus_core::constants::{
    CACHE_LINE_BYTES, MAX_SUBSCRIBERS_PER_STREAM, TOPIC_WAIT_ABI_VERSION, TOPIC_WAIT_MAGIC,
};
use rbus_core::warning::{Warning, WarningCode};

use crate::fd::OwnedFileDescriptor;
use crate::futex::futex_wake_shared;
use crate::memfd::{create_memfd, seal_file_descriptor_size};
use crate::mmap::MappedRegion;

const TOPIC_WAIT_SEGMENT_SIZE: usize = core::mem::size_of::<TopicWaitHeader>();
const TOPIC_WAIT_RESERVED_U32_COUNT: usize = 8;
const TOPIC_WAIT_HEADER_OFFSET: usize = 0;
const TOPIC_WAIT_RESERVED_HEADER: u32 = 0;
const TOPIC_WAIT_INITIAL_EPOCH: u32 = 0;
const TOPIC_WAIT_INITIAL_WAITER_MASK: u64 = 0;
const TOPIC_WAIT_RESERVED_VALUE: u32 = 0;
const COMPONENT_IPC: &str = "rbus-ipc";

/// Shared header used to wake subscribers for topic-level events
#[repr(C, align(64))]
pub struct TopicWaitHeader {
    magic: u64,
    abi_version: u32,
    reserved_header: u32,
    wake_epoch: AtomicU32,
    stream_epoch: AtomicU32,
    wake_waiters_mask: AtomicU64,
    reserved: [u32; TOPIC_WAIT_RESERVED_U32_COUNT],
}

/// Mapped topic wait segment
pub struct TopicWaitSegment {
    _mapping: MappedRegion,
    header: NonNull<TopicWaitHeader>,
}

// SAFETY: The header pointer is checked when the segment is mapped and stays valid because the
// mapping is owned by TopicWaitSegment; all shared fields are atomic
unsafe impl Send for TopicWaitSegment {}

// SAFETY: Concurrent access to TopicWaitHeader uses atomics only
unsafe impl Sync for TopicWaitSegment {}

impl TopicWaitSegment {
    /// Maps and validates a topic wait segment
    pub fn map(file_descriptor: &OwnedFileDescriptor) -> Result<Self, Warning> {
        let mapping = MappedRegion::map_shared(
            file_descriptor.as_raw_file_descriptor(),
            TOPIC_WAIT_SEGMENT_SIZE,
            true,
        )?;
        let header = mapping
            .typed_pointer::<TopicWaitHeader>(TOPIC_WAIT_HEADER_OFFSET, "TopicWaitSegment::map")?;
        validate_topic_wait_header(header)?;
        Ok(Self {
            _mapping: mapping,
            header,
        })
    }

    /// Returns the segment magic read from the mapped header
    pub fn magic(&self) -> u64 {
        self.header().magic
    }

    /// Returns the segment ABI version read from the mapped header
    pub fn abi_version(&self) -> u32 {
        self.header().abi_version
    }

    /// Returns the message wake epoch observed by subscribers
    pub fn wake_epoch(&self) -> u32 {
        self.header().wake_epoch.load(Ordering::Acquire)
    }

    /// Returns the stream-list epoch observed by subscribers
    pub fn stream_epoch(&self) -> u32 {
        self.header().stream_epoch.load(Ordering::Acquire)
    }

    /// Returns the futex word used for message wake waits
    pub fn wake_epoch_word(&self) -> &AtomicU32 {
        &self.header().wake_epoch
    }

    /// Adds a subscriber bit to the waiter mask
    pub fn add_waiter(&self, waiter_mask: u64) {
        self.header()
            .wake_waiters_mask
            .fetch_or(waiter_mask, Ordering::AcqRel);
    }

    /// Removes a subscriber bit from the waiter mask
    pub fn remove_waiter(&self, waiter_mask: u64) {
        self.header()
            .wake_waiters_mask
            .fetch_and(!waiter_mask, Ordering::AcqRel);
    }

    /// Wakes subscribers currently present in the waiter mask
    pub fn wake_waiters(&self) -> Result<(), Warning> {
        let header = self.header();
        if header.wake_waiters_mask.load(Ordering::Acquire) == 0 {
            return Ok(());
        }
        header.wake_epoch.fetch_add(1, Ordering::Release);
        let _woken_count =
            futex_wake_shared(&header.wake_epoch, u32::from(MAX_SUBSCRIBERS_PER_STREAM))?;
        Ok(())
    }

    /// Bumps the stream-list epoch and wakes subscribers
    pub fn wake_stream_added(&self) -> Result<(), Warning> {
        self.header().stream_epoch.fetch_add(1, Ordering::Release);
        self.wake_waiters()
    }

    fn header(&self) -> &TopicWaitHeader {
        // SAFETY: The header pointer was checked when this TopicWaitSegment was mapped
        unsafe { self.header.as_ref() }
    }
}

/// Creates and initializes a topic wait segment for a topic hash
pub fn create_topic_wait_segment(topic_hash: u64) -> Result<OwnedFileDescriptor, Warning> {
    let file_descriptor = create_memfd(
        &format!("rbus-topic-wait-{topic_hash}"),
        TOPIC_WAIT_SEGMENT_SIZE,
    )?;
    seal_file_descriptor_size(&file_descriptor)?;
    let mapping = MappedRegion::map_shared(
        file_descriptor.as_raw_file_descriptor(),
        TOPIC_WAIT_SEGMENT_SIZE,
        true,
    )?;
    initialize_topic_wait_segment(&mapping)?;
    Ok(file_descriptor)
}

fn initialize_topic_wait_segment(mapping: &MappedRegion) -> Result<(), Warning> {
    let header_pointer = mapping.typed_pointer::<TopicWaitHeader>(
        TOPIC_WAIT_HEADER_OFFSET,
        "initialize_topic_wait_segment",
    )?;
    // SAFETY: The header pointer was checked against the newly created mapping
    unsafe {
        core::ptr::write(
            header_pointer.as_ptr(),
            TopicWaitHeader {
                magic: TOPIC_WAIT_MAGIC,
                abi_version: TOPIC_WAIT_ABI_VERSION,
                reserved_header: TOPIC_WAIT_RESERVED_HEADER,
                wake_epoch: AtomicU32::new(TOPIC_WAIT_INITIAL_EPOCH),
                stream_epoch: AtomicU32::new(TOPIC_WAIT_INITIAL_EPOCH),
                wake_waiters_mask: AtomicU64::new(TOPIC_WAIT_INITIAL_WAITER_MASK),
                reserved: [TOPIC_WAIT_RESERVED_VALUE; TOPIC_WAIT_RESERVED_U32_COUNT],
            },
        );
    }
    Ok(())
}

fn validate_topic_wait_header(header_pointer: NonNull<TopicWaitHeader>) -> Result<(), Warning> {
    // SAFETY: The pointer was bounds and alignment checked by MappedRegion::typed_pointer
    let header = unsafe { header_pointer.as_ref() };
    if header.magic == TOPIC_WAIT_MAGIC && header.abi_version == TOPIC_WAIT_ABI_VERSION {
        Ok(())
    } else {
        Err(Warning::new(
            WarningCode::SegmentAbiMismatch,
            COMPONENT_IPC,
            "validate_topic_wait_header",
        ))
    }
}

const _: () = assert!(core::mem::size_of::<TopicWaitHeader>() == CACHE_LINE_BYTES);
