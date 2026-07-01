//! memfd_create/ftruncate/seals helpers
use std::ffi::CString;

use rbus_core::warning::{Warning, WarningCode};

use crate::errno::warning_from_errno;
use crate::fd::OwnedFileDescriptor;

const COMPONENT_IPC: &str = "rbus-ipc";

/// Creates a sealed-capable anonymous memfd and resizes it to `size` bytes
pub fn create_memfd(name: &str, size: usize) -> Result<OwnedFileDescriptor, Warning> {
    let c_name = CString::new(name).map_err(|_| {
        Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_IPC,
            "create_memfd::CString",
        )
    })?;

    // SAFETY: `c_name` is a live NUL-terminated string and the flags are constant values accepted
    // by `memfd_create`; the returned descriptor is immediately wrapped for ownership
    let raw_file_descriptor = unsafe {
        libc::syscall(
            libc::SYS_memfd_create,
            c_name.as_ptr(),
            libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
        ) as libc::c_int
    };

    let file_descriptor = OwnedFileDescriptor::new(raw_file_descriptor)?;
    resize_file_descriptor(&file_descriptor, size)?;
    Ok(file_descriptor)
}

/// Resizes a file descriptor with `ftruncate`
pub fn resize_file_descriptor(
    file_descriptor: &OwnedFileDescriptor,
    size: usize,
) -> Result<(), Warning> {
    let size_as_off_t = libc::off_t::try_from(size).map_err(|_| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_IPC,
            "resize_file_descriptor::size",
        )
    })?;

    // SAFETY: `file_descriptor` owns a live descriptor and `size_as_off_t` was range-checked
    let result =
        unsafe { libc::ftruncate(file_descriptor.as_raw_file_descriptor(), size_as_off_t) };
    if result != 0 {
        return Err(warning_from_errno(
            WarningCode::SyscallFailed,
            COMPONENT_IPC,
            "resize_file_descriptor",
        ));
    }
    Ok(())
}

/// Prevents future grow/shrink operations on a memfd
pub fn seal_file_descriptor_size(file_descriptor: &OwnedFileDescriptor) -> Result<(), Warning> {
    // SAFETY: `file_descriptor` owns a live descriptor and `F_ADD_SEALS` is called with size seals
    // supported by memfd file descriptors created with `MFD_ALLOW_SEALING`
    let result = unsafe {
        libc::fcntl(
            file_descriptor.as_raw_file_descriptor(),
            libc::F_ADD_SEALS,
            libc::F_SEAL_SHRINK | libc::F_SEAL_GROW,
        )
    };
    if result != 0 {
        return Err(warning_from_errno(
            WarningCode::SyscallFailed,
            COMPONENT_IPC,
            "seal_file_descriptor_size",
        ));
    }
    Ok(())
}
