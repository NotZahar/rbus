//! Shared memory mapping helpers

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
    /// Maps a descriptor as a shared memory region
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

        // SAFETY: The descriptor and non-zero length are validated above; the null address lets
        // the kernel choose the mapping address, and offset 0 is page-aligned
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

    /// Returns the mapped byte length
    pub const fn length(&self) -> usize {
        self.length
    }

    /// Returns an aligned typed pointer into the mapped region
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

    /// Returns a byte pointer to a checked range inside the mapping
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

    /// Borrows the entire mapping as immutable bytes
    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: `pointer` and `length` come from a successful `mmap` and remain valid while
        // `self` owns the mapping
        unsafe { core::slice::from_raw_parts(self.pointer.as_ptr(), self.length) }
    }

    /// Borrows the entire mapping as mutable bytes
    pub fn as_mutable_slice(&mut self) -> &mut [u8] {
        // SAFETY: `&mut self` guarantees a unique Rust borrow of this mapping wrapper; the
        // pointer and length come from a successful writable `mmap`
        unsafe { core::slice::from_raw_parts_mut(self.pointer.as_ptr(), self.length) }
    }
}

impl Drop for MappedRegion {
    fn drop(&mut self) {
        // SAFETY: The pointer and length are exactly the region returned by `mmap`, and `Drop`
        // runs once for the owner of this mapping
        unsafe {
            libc::munmap(self.pointer.as_ptr().cast(), self.length);
        }
    }
}

// SAFETY: The wrapper owns an OS mapping; moving it to another thread does not invalidate it
unsafe impl Send for MappedRegion {}
// SAFETY: Shared access to the mapping is allowed; data synchronization is handled by callers and
// the shared-memory atomic protocol
unsafe impl Sync for MappedRegion {}
