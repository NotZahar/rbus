//! Linux IPC metadata

use std::os::fd::RawFd;

use rbus_core::result::RbusResult;
use rbus_core::warning::{Warning, WarningCode};

use crate::errno::warning_from_errno;
use crate::fd::OwnedFileDescriptor;

const COMPONENT_IPC: &str = "rbus-ipc";

/// Internal documentation
#[derive(Debug)]
pub struct ReceivedFileDescriptors {
    /// Payload metadata
    pub byte_count: usize,
    /// Linux IPC metadata
    pub file_descriptors: Vec<OwnedFileDescriptor>,
}

/// Payload metadata
pub fn send_file_descriptors(
    socket_file_descriptor: RawFd,
    payload: &[u8],
    file_descriptors: &[RawFd],
) -> RbusResult<usize> {
    validate_send_arguments(socket_file_descriptor, payload, file_descriptors)?;

    let mut io_vector = libc::iovec {
        iov_base: payload.as_ptr().cast_mut().cast(),
        iov_len: payload.len(),
    };
    let mut control_buffer = if file_descriptors.is_empty() {
        Vec::new()
    } else {
        vec![0_u8; control_buffer_len(file_descriptors.len())?]
    };
    let mut message_header = new_message_header(&mut io_vector, &mut control_buffer);
    if !file_descriptors.is_empty() {
        write_rights_control_message(&mut message_header, file_descriptors)?;
    }

    // SAFETY: Required invariants are upheld
    // Invariants:
    // - Required invariant holds
    // - Required invariant holds
    // Covered by tests:
    // - scm_rights_sends_and_receives_file_descriptor
    let result = unsafe { libc::sendmsg(socket_file_descriptor, &message_header, 0) };
    if result < 0 {
        return Err(warning_from_errno(
            WarningCode::SyscallFailed,
            COMPONENT_IPC,
            "send_file_descriptors",
        ));
    }

    usize::try_from(result).map_err(|_| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_IPC,
            "send_file_descriptors::result",
        )
    })
}

/// Payload metadata
pub fn receive_file_descriptors(
    socket_file_descriptor: RawFd,
    payload_buffer: &mut [u8],
    max_file_descriptors: usize,
) -> RbusResult<ReceivedFileDescriptors> {
    validate_receive_arguments(socket_file_descriptor, payload_buffer, max_file_descriptors)?;

    let mut io_vector = libc::iovec {
        iov_base: payload_buffer.as_mut_ptr().cast(),
        iov_len: payload_buffer.len(),
    };
    let mut control_buffer = vec![0_u8; control_buffer_len(max_file_descriptors)?];
    let mut message_header = new_message_header(&mut io_vector, &mut control_buffer);

    // SAFETY: Required invariants are upheld
    // Invariants:
    // - Required invariant holds
    // - Required invariant holds
    // Covered by tests:
    // - scm_rights_sends_and_receives_file_descriptor
    let result = unsafe { libc::recvmsg(socket_file_descriptor, &mut message_header, 0) };
    if result < 0 {
        return Err(warning_from_errno(
            WarningCode::SyscallFailed,
            COMPONENT_IPC,
            "receive_file_descriptors",
        ));
    }
    if message_header.msg_flags & libc::MSG_CTRUNC != 0 {
        return Err(Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_IPC,
            "receive_file_descriptors::truncated_control",
        ));
    }

    let byte_count = usize::try_from(result).map_err(|_| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_IPC,
            "receive_file_descriptors::result",
        )
    })?;
    let file_descriptors = collect_rights_control_messages(&message_header)?;

    Ok(ReceivedFileDescriptors {
        byte_count,
        file_descriptors,
    })
}

fn validate_send_arguments(
    socket_file_descriptor: RawFd,
    payload: &[u8],
    file_descriptors: &[RawFd],
) -> RbusResult<()> {
    if socket_file_descriptor < 0 || payload.is_empty() {
        return Err(Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_IPC,
            "send_file_descriptors::arguments",
        ));
    }
    if file_descriptors
        .iter()
        .any(|file_descriptor| *file_descriptor < 0)
    {
        return Err(Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_IPC,
            "send_file_descriptors::fd",
        ));
    }
    Ok(())
}

fn validate_receive_arguments(
    socket_file_descriptor: RawFd,
    payload_buffer: &[u8],
    max_file_descriptors: usize,
) -> RbusResult<()> {
    if socket_file_descriptor < 0 || payload_buffer.is_empty() || max_file_descriptors == 0 {
        return Err(Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_IPC,
            "receive_file_descriptors::arguments",
        ));
    }
    Ok(())
}

fn control_buffer_len(file_descriptor_count: usize) -> RbusResult<usize> {
    let descriptor_bytes = file_descriptor_count
        .checked_mul(core::mem::size_of::<RawFd>())
        .ok_or_else(|| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_IPC,
                "control_buffer_len::descriptor_bytes",
            )
        })?;
    let descriptor_bytes = libc::c_uint::try_from(descriptor_bytes).map_err(|_| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_IPC,
            "control_buffer_len::c_uint",
        )
    })?;

    Ok(cmsg_space(descriptor_bytes))
}

fn new_message_header<'a>(
    io_vector: &'a mut libc::iovec,
    control_buffer: &'a mut [u8],
) -> libc::msghdr {
    let (control_pointer, control_length) = if control_buffer.is_empty() {
        (core::ptr::null_mut(), 0)
    } else {
        (control_buffer.as_mut_ptr().cast(), control_buffer.len())
    };
    libc::msghdr {
        msg_name: core::ptr::null_mut(),
        msg_namelen: 0,
        msg_iov: io_vector,
        msg_iovlen: 1,
        msg_control: control_pointer,
        msg_controllen: control_length,
        msg_flags: 0,
    }
}

fn write_rights_control_message(
    message_header: &mut libc::msghdr,
    file_descriptors: &[RawFd],
) -> RbusResult<()> {
    let descriptor_bytes = file_descriptors
        .len()
        .checked_mul(core::mem::size_of::<RawFd>())
        .and_then(|value| libc::c_uint::try_from(value).ok())
        .ok_or_else(|| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_IPC,
                "write_rights_control_message::descriptor_bytes",
            )
        })?;

    // SAFETY: Required invariants are upheld
    // Invariants:
    // - Required invariant holds
    // - Required invariant holds
    // Covered by tests:
    // - scm_rights_sends_and_receives_file_descriptor
    let control_message = unsafe { libc::CMSG_FIRSTHDR(message_header) };
    if control_message.is_null() {
        return Err(Warning::new(
            WarningCode::InternalInvariantWarning,
            COMPONENT_IPC,
            "write_rights_control_message::first_header",
        ));
    }

    // SAFETY: Required invariants are upheld
    // Invariants:
    // - Required invariant holds
    // - Required invariant holds
    // Covered by tests:
    // - scm_rights_sends_and_receives_file_descriptor
    unsafe {
        (*control_message).cmsg_level = libc::SOL_SOCKET;
        (*control_message).cmsg_type = libc::SCM_RIGHTS;
        (*control_message).cmsg_len = cmsg_len(descriptor_bytes);
        core::ptr::copy_nonoverlapping(
            file_descriptors.as_ptr(),
            libc::CMSG_DATA(control_message).cast(),
            file_descriptors.len(),
        );
    }

    Ok(())
}

fn collect_rights_control_messages(
    message_header: &libc::msghdr,
) -> RbusResult<Vec<OwnedFileDescriptor>> {
    let mut received_file_descriptors = Vec::new();

    // SAFETY: Required invariants are upheld
    // Invariants:
    // - Required invariant holds
    // - Required invariant holds
    // Covered by tests:
    // - scm_rights_sends_and_receives_file_descriptor
    unsafe {
        let mut control_message = libc::CMSG_FIRSTHDR(message_header);
        while !control_message.is_null() {
            if (*control_message).cmsg_level == libc::SOL_SOCKET
                && (*control_message).cmsg_type == libc::SCM_RIGHTS
            {
                append_file_descriptors_from_control_message(
                    control_message,
                    &mut received_file_descriptors,
                )?;
            }
            control_message = libc::CMSG_NXTHDR(message_header, control_message);
        }
    }

    Ok(received_file_descriptors)
}

unsafe fn append_file_descriptors_from_control_message(
    control_message: *const libc::cmsghdr,
    received_file_descriptors: &mut Vec<OwnedFileDescriptor>,
) -> RbusResult<()> {
    let header_len = cmsg_len(0);
    // SAFETY: Required invariants are upheld
    // Invariants:
    // - Required invariant holds
    // - Required invariant holds
    // Covered by tests:
    // - scm_rights_sends_and_receives_file_descriptor
    let control_message_len = unsafe { (*control_message).cmsg_len };
    if control_message_len < header_len {
        return Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_IPC,
            "append_file_descriptors_from_control_message::len",
        ));
    }

    let payload_len = control_message_len - header_len;
    if !payload_len.is_multiple_of(core::mem::size_of::<RawFd>()) {
        return Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_IPC,
            "append_file_descriptors_from_control_message::payload_len",
        ));
    }
    let descriptor_count = payload_len / core::mem::size_of::<RawFd>();
    // SAFETY: Required invariants are upheld
    // Invariants:
    // - Required invariant holds
    // - Required invariant holds
    // Covered by tests:
    // - scm_rights_sends_and_receives_file_descriptor
    let descriptor_pointer = unsafe { libc::CMSG_DATA(control_message.cast_mut()).cast::<RawFd>() };

    for descriptor_index in 0..descriptor_count {
        // SAFETY: Required invariants are upheld
        // Invariants:
        // - Required invariant holds
        // - Required invariant holds
        // Covered by tests:
        // - scm_rights_sends_and_receives_file_descriptor
        let raw_file_descriptor = unsafe { *descriptor_pointer.add(descriptor_index) };
        received_file_descriptors.push(OwnedFileDescriptor::new(raw_file_descriptor)?);
    }

    Ok(())
}

fn cmsg_space(length: libc::c_uint) -> usize {
    // SAFETY: Required invariants are upheld
    // Invariants:
    // - Required invariant holds
    // - Required invariant holds
    // Covered by tests:
    // - scm_rights_sends_and_receives_file_descriptor
    unsafe { libc::CMSG_SPACE(length) as usize }
}

fn cmsg_len(length: libc::c_uint) -> usize {
    // SAFETY: Required invariants are upheld
    // Invariants:
    // - Required invariant holds
    // - Required invariant holds
    // Covered by tests:
    // - scm_rights_sends_and_receives_file_descriptor
    unsafe { libc::CMSG_LEN(length) as usize }
}
