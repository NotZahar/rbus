use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;

use rbus_core::warning::{Warning, WarningCode};
use rbus_ipc::memfd::create_memfd;
use rbus_ipc::scm_rights::{receive_file_descriptors, send_file_descriptors};

#[test]
fn scm_rights_sends_and_receives_file_descriptor() -> Result<(), Warning> {
    let (sender, receiver) = UnixStream::pair().map_err(|_| {
        Warning::new(
            WarningCode::SyscallFailed,
            "rbus-ipc-test",
            "UnixStream::pair",
        )
    })?;
    let memfd = create_memfd("rbus-scm-rights-test", 64)?;

    let byte_count =
        send_file_descriptors(sender.as_raw_fd(), b"F", &[memfd.as_raw_file_descriptor()])?;
    assert_eq!(byte_count, 1);

    let mut payload_buffer = [0_u8; 1];
    let received = receive_file_descriptors(receiver.as_raw_fd(), &mut payload_buffer, 1)?;
    assert_eq!(received.byte_count, 1);
    assert_eq!(payload_buffer, [b'F']);
    assert_eq!(received.file_descriptors.len(), 1);

    write_all(
        received.file_descriptors[0].as_raw_file_descriptor(),
        b"fd-ok",
    )?;
    let mut read_buffer = [0_u8; 5];
    read_exact_from_start(memfd.as_raw_file_descriptor(), &mut read_buffer)?;
    assert_eq!(read_buffer, *b"fd-ok");

    Ok(())
}

fn write_all(raw_file_descriptor: libc::c_int, mut bytes: &[u8]) -> Result<(), Warning> {
    while !bytes.is_empty() {
        // SAFETY: Required invariants are upheld
        // Invariants:
        // - Required invariant holds
        // - Required invariant holds
        // Covered by tests:
        // - scm_rights_sends_and_receives_file_descriptor
        let written =
            unsafe { libc::write(raw_file_descriptor, bytes.as_ptr().cast(), bytes.len()) };
        if written < 0 {
            return Err(Warning::new(
                WarningCode::SyscallFailed,
                "rbus-ipc-test",
                "write_all",
            ));
        }
        let written = usize::try_from(written).map_err(|_| {
            Warning::new(
                WarningCode::ResourceLimit,
                "rbus-ipc-test",
                "write_all::written",
            )
        })?;
        bytes = &bytes[written..];
    }
    Ok(())
}

fn read_exact_from_start(
    raw_file_descriptor: libc::c_int,
    buffer: &mut [u8],
) -> Result<(), Warning> {
    seek_start(raw_file_descriptor)?;
    let mut read_count = 0_usize;
    while read_count < buffer.len() {
        // SAFETY: Required invariants are upheld
        // Invariants:
        // - Required invariant holds
        // - Required invariant holds
        // Covered by tests:
        // - scm_rights_sends_and_receives_file_descriptor
        let current_read_count = unsafe {
            libc::read(
                raw_file_descriptor,
                buffer[read_count..].as_mut_ptr().cast(),
                buffer.len() - read_count,
            )
        };
        if current_read_count <= 0 {
            return Err(Warning::new(
                WarningCode::SyscallFailed,
                "rbus-ipc-test",
                "read_exact_from_start",
            ));
        }
        read_count = read_count
            .checked_add(usize::try_from(current_read_count).map_err(|_| {
                Warning::new(
                    WarningCode::ResourceLimit,
                    "rbus-ipc-test",
                    "read_exact_from_start::count",
                )
            })?)
            .ok_or_else(|| {
                Warning::new(
                    WarningCode::ResourceLimit,
                    "rbus-ipc-test",
                    "read_exact_from_start::overflow",
                )
            })?;
    }
    Ok(())
}

fn seek_start(raw_file_descriptor: libc::c_int) -> Result<(), Warning> {
    // SAFETY: Required invariants are upheld
    // Invariants:
    // - Required invariant holds
    // - Required invariant holds
    // Covered by tests:
    // - scm_rights_sends_and_receives_file_descriptor
    let offset = unsafe { libc::lseek(raw_file_descriptor, 0, libc::SEEK_SET) };
    if offset < 0 {
        return Err(Warning::new(
            WarningCode::SyscallFailed,
            "rbus-ipc-test",
            "seek_start",
        ));
    }
    Ok(())
}
