//! Linux IPC metadata

use core::mem::{align_of, size_of};
use core::ptr::NonNull;
use std::os::fd::RawFd;

use rbus_core::warning::{Warning, WarningCode};

use crate::errno::warning_from_errno;

const COMPONENT_IPC: &str = "rbus-ipc";
const MINIMUM_FILE_DESCRIPTOR: RawFd = 0;
const EMPTY_LENGTH: usize = 0;

/// Mapped shared memory region
#[derive(Debug)]
pub struct MappedRegion {
    pointer: NonNull<u8>,
    length: usize,
}

impl MappedRegion {
    /// Linux IPC metadata
    pub fn map_shared(
        raw_file_descriptor: RawFd,
        length: usize,
        writable: bool,
    ) -> Result<Self, Warning> {
        if raw_file_descriptor < MINIMUM_FILE_DESCRIPTOR || length == EMPTY_LENGTH {
            return Err(Warning::new(
                WarningCode::InvalidConfig,
                COMPONENT_IPC,
                "MappedRegion::map_shared",
            ));
        }

        let protection = if writable {
            libc::PROT_READ | libc::PROT_WRITE
        } else {
            libc::PROT_READ
        };

        // SAFETY: Required invariants are upheld
        // Invariants:
        // - Required invariant holds
        // - Required invariant holds
        // Covered by tests:
        // - mmap_region_allows_shared_read_write
        let raw_pointer = unsafe {
            libc::mmap(
                core::ptr::null_mut(),
                length,
                protection,
                libc::MAP_SHARED,
                raw_file_descriptor,
                0,
            )
        };

        if raw_pointer == libc::MAP_FAILED {
            return Err(warning_from_errno(
                WarningCode::MmapFailed,
                COMPONENT_IPC,
                "MappedRegion::map_shared",
            ));
        }

        let pointer = match NonNull::new(raw_pointer.cast::<u8>()) {
            Some(pointer) => pointer,
            None => {
                return Err(Warning::new(
                    WarningCode::MmapFailed,
                    COMPONENT_IPC,
                    "MappedRegion::map_shared::null",
                ));
            }
        };

        Ok(Self { pointer, length })
    }

    /// Returns a value
    pub const fn length(&self) -> usize {
        self.length
    }

    /// Linux IPC metadata
    pub fn typed_pointer<T>(
        &self,
        offset: usize,
        operation: &'static str,
    ) -> Result<NonNull<T>, Warning> {
        self.byte_pointer(offset, size_of::<T>(), operation)
            .and_then(|pointer| {
                if (pointer.as_ptr().cast::<T>() as usize).is_multiple_of(align_of::<T>()) {
                    Ok(pointer.cast())
                } else {
                    Err(Warning::new(
                        WarningCode::SegmentAbiMismatch,
                        COMPONENT_IPC,
                        operation,
                    ))
                }
            })
    }

    /// Linux IPC metadata
    pub fn byte_pointer(
        &self,
        offset: usize,
        length: usize,
        operation: &'static str,
    ) -> Result<NonNull<u8>, Warning> {
        let end = offset
            .checked_add(length)
            .ok_or_else(|| Warning::new(WarningCode::ResourceLimit, COMPONENT_IPC, operation))?;
        if length == EMPTY_LENGTH || end > self.length {
            return Err(Warning::new(
                WarningCode::SegmentAbiMismatch,
                COMPONENT_IPC,
                operation,
            ));
        }
        NonNull::new(self.pointer.as_ptr().wrapping_add(offset)).ok_or_else(|| {
            Warning::new(
                WarningCode::InternalInvariantWarning,
                COMPONENT_IPC,
                operation,
            )
        })
    }

    /// Returns a value
    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: Required invariants are upheld
        // Invariants:
        // - Required invariant holds
        // - Required invariant holds
        // Covered by tests:
        // - mmap_region_allows_shared_read_write
        unsafe { core::slice::from_raw_parts(self.pointer.as_ptr(), self.length) }
    }

    /// Returns a value
    pub fn as_mutable_slice(&mut self) -> &mut [u8] {
        // SAFETY: Required invariants are upheld
        // Invariants:
        // - Required invariant holds
        // - Required invariant holds
        // Covered by tests:
        // - mmap_region_allows_shared_read_write
        unsafe { core::slice::from_raw_parts_mut(self.pointer.as_ptr(), self.length) }
    }
}

impl Drop for MappedRegion {
    fn drop(&mut self) {
        // SAFETY: Required invariants are upheld
        // Invariants:
        // - Required invariant holds
        // - Required invariant holds
        // Covered by tests:
        // - mmap_region_allows_shared_read_write
        unsafe {
            libc::munmap(self.pointer.as_ptr().cast(), self.length);
        }
    }
}

// SAFETY: Required invariants are upheld
unsafe impl Send for MappedRegion {}
// SAFETY: Required invariants are upheld
unsafe impl Sync for MappedRegion {}
