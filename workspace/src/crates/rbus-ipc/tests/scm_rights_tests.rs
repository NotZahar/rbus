use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::net::UnixStream;

use rbus_core::warning::{Warning, WarningCode};
use rbus_ipc::control::io::{
    read_control_frame, read_control_frame_with_file_descriptors,
    write_control_frame_with_file_descriptors,
};
use rbus_ipc::control::{FrameHeader, Opcode};
use rbus_ipc::memfd::create_memfd;
use rbus_ipc::scm_rights::{receive_file_descriptors, send_file_descriptors};

const CONTROL_TEST_FLAGS: u16 = 0;
const CONTROL_TEST_PAYLOAD: &[u8] = b"frame";
const CONTROL_TEST_PAYLOAD_LENGTH: u32 = 5;
const INVALID_FILE_DESCRIPTOR: RawFd = -1;
const MAX_CONTROL_FILE_DESCRIPTORS: usize = 1;
const MAX_CONTROL_PAYLOAD_LENGTH: usize = 16;
const SEQPACKET_RECEIVER_INDEX: usize = 1;
const SEQPACKET_SENDER_INDEX: usize = 0;
const SOCKET_PAIR_DESCRIPTOR_COUNT: usize = 2;
const SOCKET_PROTOCOL_DEFAULT: i32 = 0;
const SUCCESS_RETURN_CODE: i32 = 0;
const TEST_MEMFD_SIZE: usize = 64;

#[test]
fn scm_rights_sends_and_receives_file_descriptor() -> Result<(), Warning> {
    let (sender, receiver) = seqpacket_pair()?;
    let memfd = create_memfd("rbus-scm-rights-test", TEST_MEMFD_SIZE)?;

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

#[test]
fn control_frame_with_file_descriptors_roundtrips_on_seqpacket() -> Result<(), Warning> {
    let (mut sender, mut receiver) = seqpacket_pair()?;
    let memfd = create_memfd("rbus-control-rights-test", TEST_MEMFD_SIZE)?;
    let header = FrameHeader::new(
        Opcode::AttachStreams,
        CONTROL_TEST_FLAGS,
        CONTROL_TEST_PAYLOAD_LENGTH,
    );

    write_control_frame_with_file_descriptors(
        &mut sender,
        header,
        CONTROL_TEST_PAYLOAD,
        &[memfd.as_raw_file_descriptor()],
    )?;
    let received = read_control_frame_with_file_descriptors(
        &mut receiver,
        MAX_CONTROL_PAYLOAD_LENGTH,
        MAX_CONTROL_FILE_DESCRIPTORS,
    )?;

    assert_eq!(received.frame.header, header);
    assert_eq!(received.frame.payload, CONTROL_TEST_PAYLOAD);
    assert_eq!(
        received.file_descriptors.len(),
        MAX_CONTROL_FILE_DESCRIPTORS
    );
    Ok(())
}

#[test]
fn control_frame_without_file_descriptors_roundtrips_on_plain_reader() -> Result<(), Warning> {
    let (mut sender, mut receiver) = seqpacket_pair()?;
    let header = FrameHeader::new(
        Opcode::Inspect,
        CONTROL_TEST_FLAGS,
        CONTROL_TEST_PAYLOAD_LENGTH,
    );

    write_control_frame_with_file_descriptors(&mut sender, header, CONTROL_TEST_PAYLOAD, &[])?;
    let received = read_control_frame(&mut receiver, MAX_CONTROL_PAYLOAD_LENGTH)?;

    assert_eq!(received.header, header);
    assert_eq!(received.payload, CONTROL_TEST_PAYLOAD);
    Ok(())
}

#[test]
fn control_frame_without_file_descriptors_roundtrips_on_fd_reader() -> Result<(), Warning> {
    let (mut sender, mut receiver) = seqpacket_pair()?;
    let header = FrameHeader::new(
        Opcode::NotifyStreamAdded,
        CONTROL_TEST_FLAGS,
        CONTROL_TEST_PAYLOAD_LENGTH,
    );

    write_control_frame_with_file_descriptors(&mut sender, header, CONTROL_TEST_PAYLOAD, &[])?;
    let received = read_control_frame_with_file_descriptors(
        &mut receiver,
        MAX_CONTROL_PAYLOAD_LENGTH,
        MAX_CONTROL_FILE_DESCRIPTORS,
    )?;

    assert_eq!(received.frame.header, header);
    assert_eq!(received.frame.payload, CONTROL_TEST_PAYLOAD);
    assert!(received.file_descriptors.is_empty());
    Ok(())
}

fn seqpacket_pair() -> Result<(UnixStream, UnixStream), Warning> {
    let mut file_descriptors = [INVALID_FILE_DESCRIPTOR; SOCKET_PAIR_DESCRIPTOR_COUNT];
    // SAFETY: socketpair initializes both descriptors in file_descriptors on success
    let result = unsafe {
        libc::socketpair(
            libc::AF_UNIX,
            libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
            SOCKET_PROTOCOL_DEFAULT,
            file_descriptors.as_mut_ptr(),
        )
    };
    if result != SUCCESS_RETURN_CODE {
        return Err(Warning::new(
            WarningCode::SyscallFailed,
            "rbus-ipc-test",
            "seqpacket_pair",
        ));
    }
    // SAFETY: both descriptors are newly created connected SOCK_SEQPACKET endpoints
    let sender = unsafe { UnixStream::from_raw_fd(file_descriptors[SEQPACKET_SENDER_INDEX]) };
    // SAFETY: both descriptors are newly created connected SOCK_SEQPACKET endpoints
    let receiver = unsafe { UnixStream::from_raw_fd(file_descriptors[SEQPACKET_RECEIVER_INDEX]) };
    Ok((sender, receiver))
}

fn write_all(raw_file_descriptor: libc::c_int, mut bytes: &[u8]) -> Result<(), Warning> {
    while !bytes.is_empty() {
        // SAFETY: `bytes` is a live input slice and `raw_file_descriptor` is supplied by the test
        // as an open descriptor
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
        // SAFETY: The remaining buffer slice is valid for writes, and the descriptor is open for
        // reading in this test helper
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
    // SAFETY: `lseek` receives an open descriptor supplied by the test and uses constant seek
    // parameters
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
