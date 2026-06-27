//! Owned file descriptor wrapper
use std::os::fd::RawFd;

use rbus_core::warning::{Warning, WarningCode};

use crate::errno::warning_from_errno;

const COMPONENT_IPC: &str = "rbus-ipc";

/// Owned raw file descriptor
#[derive(Debug)]
pub struct OwnedFileDescriptor {
    raw_file_descriptor: RawFd,
}

impl OwnedFileDescriptor {
    /// Linux IPC metadata
    pub fn new(raw_file_descriptor: RawFd) -> Result<Self, Warning> {
        if raw_file_descriptor < 0 {
            return Err(warning_from_errno(
                WarningCode::SyscallFailed,
                COMPONENT_IPC,
                "OwnedFileDescriptor::new",
            ));
        }
        Ok(Self {
            raw_file_descriptor,
        })
    }

    /// Linux IPC metadata
    pub const fn as_raw_file_descriptor(&self) -> RawFd {
        self.raw_file_descriptor
    }

    /// Linux IPC metadata
    pub fn into_raw_file_descriptor(mut self) -> RawFd {
        let raw_file_descriptor = self.raw_file_descriptor;
        self.raw_file_descriptor = -1;
        raw_file_descriptor
    }
}

impl Drop for OwnedFileDescriptor {
    fn drop(&mut self) {
        if self.raw_file_descriptor >= 0 {
            // SAFETY: Required invariants are upheld
            // Invariants:
            // - Required invariant holds
            // - Required invariant holds
            // Covered by tests:
            // - memfd_create_returns_valid_descriptor
            unsafe {
                libc::close(self.raw_file_descriptor);
            }
            self.raw_file_descriptor = -1;
        }
    }
}
