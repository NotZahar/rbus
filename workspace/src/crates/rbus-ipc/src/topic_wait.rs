//! Topic-level futex wait segment
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use rbus_core::constants::{CACHE_LINE_BYTES, MAX_SUBSCRIBERS_PER_STREAM};
use rbus_core::warning::Warning;

use crate::fd::OwnedFileDescriptor;
use crate::futex::futex_wake_shared;
use crate::memfd::{create_memfd, seal_file_descriptor_size};
use crate::mmap::MappedRegion;

const TOPIC_WAIT_SEGMENT_SIZE: usize = core::mem::size_of::<TopicWaitHeader>();
const TOPIC_WAIT_RESERVED_U32_COUNT: usize = 12;

/// Topic runtime metadata
#[repr(C, align(64))]
pub struct TopicWaitHeader {
    wake_epoch: AtomicU32,
    stream_epoch: AtomicU32,
    wake_waiters_mask: AtomicU64,
    reserved: [u32; TOPIC_WAIT_RESERVED_U32_COUNT],
}

/// Mapped topic wait segment
pub struct TopicWaitSegment {
    mapping: MappedRegion,
}

impl TopicWaitSegment {
    /// Topic runtime metadata
    pub fn map(file_descriptor: &OwnedFileDescriptor) -> Result<Self, Warning> {
        Ok(Self {
            mapping: MappedRegion::map_shared(
                file_descriptor.as_raw_file_descriptor(),
                TOPIC_WAIT_SEGMENT_SIZE,
                true,
            )?,
        })
    }

    /// Returns a value
    pub fn wake_epoch(&self) -> u32 {
        self.header().wake_epoch.load(Ordering::Acquire)
    }

    /// Stream runtime metadata
    pub fn stream_epoch(&self) -> u32 {
        self.header().stream_epoch.load(Ordering::Acquire)
    }

    /// Linux IPC metadata
    pub fn wake_epoch_word(&self) -> &AtomicU32 {
        &self.header().wake_epoch
    }

    /// Internal documentation
    pub fn add_waiter(&self, waiter_mask: u64) {
        self.header()
            .wake_waiters_mask
            .fetch_or(waiter_mask, Ordering::AcqRel);
    }

    /// Internal documentation
    pub fn remove_waiter(&self, waiter_mask: u64) {
        self.header()
            .wake_waiters_mask
            .fetch_and(!waiter_mask, Ordering::AcqRel);
    }

    /// Subscriber runtime metadata
    pub fn wake_waiters(&self) -> Result<(), Warning> {
        let header = self.header();
        header.wake_epoch.fetch_add(1, Ordering::Release);
        if header.wake_waiters_mask.load(Ordering::Acquire) != 0 {
            let _woken_count =
                futex_wake_shared(&header.wake_epoch, u32::from(MAX_SUBSCRIBERS_PER_STREAM))?;
        }
        Ok(())
    }

    /// Subscriber runtime metadata
    pub fn wake_stream_added(&self) -> Result<(), Warning> {
        self.header().stream_epoch.fetch_add(1, Ordering::Release);
        self.wake_waiters()
    }

    fn header(&self) -> &TopicWaitHeader {
        // SAFETY: Required invariants are upheld
        unsafe { &*self.mapping.as_pointer().cast::<TopicWaitHeader>() }
    }
}

/// Topic runtime metadata
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
    initialize_topic_wait_segment(&mapping);
    Ok(file_descriptor)
}

fn initialize_topic_wait_segment(mapping: &MappedRegion) {
    // SAFETY: Required invariants are upheld
    unsafe {
        core::ptr::write(
            mapping.as_mutable_pointer().cast::<TopicWaitHeader>(),
            TopicWaitHeader {
                wake_epoch: AtomicU32::new(0),
                stream_epoch: AtomicU32::new(0),
                wake_waiters_mask: AtomicU64::new(0),
                reserved: [0; TOPIC_WAIT_RESERVED_U32_COUNT],
            },
        );
    }
}

const _: () = assert!(core::mem::size_of::<TopicWaitHeader>() == CACHE_LINE_BYTES);
