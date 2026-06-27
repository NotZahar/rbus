//! Linux IPC metadata

use core::ptr::NonNull;
use std::os::fd::RawFd;

use rbus_core::warning::{Warning, WarningCode};

use crate::errno::warning_from_errno;

const COMPONENT_IPC: &str = "rbus-ipc";

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
        if raw_file_descriptor < 0 || length == 0 {
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

    /// Returns a value
    pub const fn as_pointer(&self) -> *const u8 {
        self.pointer.as_ptr()
    }

    /// Returns a value
    pub const fn as_mutable_pointer(&self) -> *mut u8 {
        self.pointer.as_ptr()
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
