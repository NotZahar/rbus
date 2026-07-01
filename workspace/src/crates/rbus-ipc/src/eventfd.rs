//! eventfd wait/wake primitive
use rbus_core::warning::{Warning, WarningCode};

use crate::errno::warning_from_errno;
use crate::fd::OwnedFileDescriptor;

const COMPONENT_IPC: &str = "rbus-ipc";

/// Creates a non-blocking eventfd used for wake notifications
pub fn create_eventfd() -> Result<OwnedFileDescriptor, Warning> {
    // SAFETY: `eventfd` is called with constant flags and an initial counter value of zero
    let raw_file_descriptor = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
    OwnedFileDescriptor::new(raw_file_descriptor)
}

/// Writes one wake token to an eventfd
pub fn wake_eventfd(file_descriptor: &OwnedFileDescriptor) -> Result<(), Warning> {
    let value: u64 = 1;
    // SAFETY: `value` is a live 8-byte buffer as required by eventfd writes, and the descriptor is
    // owned by `file_descriptor`
    let result = unsafe {
        libc::write(
            file_descriptor.as_raw_file_descriptor(),
            core::ptr::addr_of!(value).cast(),
            core::mem::size_of::<u64>(),
        )
    };
    if result < 0 {
        return Err(warning_from_errno(
            WarningCode::SyscallFailed,
            COMPONENT_IPC,
            "wake_eventfd",
        ));
    }
    Ok(())
}
