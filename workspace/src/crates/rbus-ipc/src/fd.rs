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
    /// Takes ownership of a non-negative raw file descriptor
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

    /// Returns the raw descriptor without transferring ownership
    pub const fn as_raw_file_descriptor(&self) -> RawFd {
        self.raw_file_descriptor
    }

    /// Transfers ownership of the raw descriptor to the caller
    pub fn into_raw_file_descriptor(mut self) -> RawFd {
        let raw_file_descriptor = self.raw_file_descriptor;
        self.raw_file_descriptor = -1;
        raw_file_descriptor
    }
}

impl Drop for OwnedFileDescriptor {
    fn drop(&mut self) {
        if self.raw_file_descriptor >= 0 {
            // SAFETY: The descriptor is owned by this wrapper and is closed at most once because
            // it is set to -1 immediately after the syscall
            unsafe {
                libc::close(self.raw_file_descriptor);
            }
            self.raw_file_descriptor = -1;
        }
    }
}
