//! Read and write helpers for control protocol frames

use std::io::{Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;

use rbus_core::warning::{Warning, WarningCode};

use crate::fd::OwnedFileDescriptor;
use crate::scm_rights::{receive_file_descriptors, send_file_descriptors};

use super::{FrameHeader, decode_frame_header, encode_frame_header};

const COMPONENT_IPC: &str = "rbus-ipc";

/// Frame flag indicating that the payload is JSON
pub const CONTROL_FLAG_JSON: u16 = 1;

/// Decoded control frame without attached file descriptors
#[derive(Debug, Eq, PartialEq)]
pub struct ControlFrame {
    /// Fixed-size control frame header
    pub header: FrameHeader,
    /// Frame payload bytes
    pub payload: Vec<u8>,
}

/// Decoded control frame with received file descriptors
#[derive(Debug)]
pub struct ControlFrameWithFileDescriptors {
    /// Fixed-size header and payload bytes
    pub frame: ControlFrame,
    /// File descriptors received through `SCM_RIGHTS`
    pub file_descriptors: Vec<OwnedFileDescriptor>,
}

/// Writes one control frame to a connected Unix stream
pub fn write_control_frame(
    stream: &mut UnixStream,
    header: FrameHeader,
    payload: &[u8],
) -> Result<(), Warning> {
    let payload_length = u32::try_from(payload.len()).map_err(|_| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_IPC,
            "write_control_frame::payload_len",
        )
    })?;
    if header.payload_length != payload_length {
        return Err(Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_IPC,
            "write_control_frame::header_len",
        ));
    }

    let mut header_buffer = [0_u8; FrameHeader::ENCODED_SIZE];
    encode_frame_header(header, &mut header_buffer)?;
    stream
        .write_all(&header_buffer)
        .map_err(|error| warning_from_io(error, "write_control_frame::header"))?;
    stream
        .write_all(payload)
        .map_err(|error| warning_from_io(error, "write_control_frame::payload"))
}

/// Writes one control frame and optional file descriptors in a single socket message
pub fn write_control_frame_with_file_descriptors(
    stream: &mut UnixStream,
    header: FrameHeader,
    payload: &[u8],
    file_descriptors: &[RawFd],
) -> Result<(), Warning> {
    let payload_length = u32::try_from(payload.len()).map_err(|_| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_IPC,
            "write_control_frame_with_file_descriptors::payload_len",
        )
    })?;
    if header.payload_length != payload_length {
        return Err(Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_IPC,
            "write_control_frame_with_file_descriptors::header_len",
        ));
    }
    let frame_length = FrameHeader::ENCODED_SIZE
        .checked_add(payload.len())
        .ok_or_else(|| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_IPC,
                "write_control_frame_with_file_descriptors::frame_len",
            )
        })?;
    let mut frame_buffer = vec![0_u8; frame_length];
    encode_frame_header(header, &mut frame_buffer[..FrameHeader::ENCODED_SIZE])?;
    frame_buffer[FrameHeader::ENCODED_SIZE..].copy_from_slice(payload);
    let written = send_file_descriptors(stream.as_raw_fd(), &frame_buffer, file_descriptors)?;
    if written == frame_buffer.len() {
        Ok(())
    } else {
        Err(Warning::new(
            WarningCode::Disconnected,
            COMPONENT_IPC,
            "write_control_frame_with_file_descriptors::short_write",
        ))
    }
}

/// Reads one control frame from a connected Unix stream
pub fn read_control_frame(
    stream: &mut UnixStream,
    max_payload_length: usize,
) -> Result<ControlFrame, Warning> {
    let buffer_length = FrameHeader::ENCODED_SIZE
        .checked_add(max_payload_length)
        .ok_or_else(|| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_IPC,
                "read_control_frame::buffer_len",
            )
        })?;
    let mut frame_buffer = vec![0_u8; buffer_length];
    let byte_count = stream
        .read(&mut frame_buffer)
        .map_err(|error| warning_from_io(error, "read_control_frame::frame"))?;
    if byte_count == 0 {
        return Err(Warning::new(
            WarningCode::Disconnected,
            COMPONENT_IPC,
            "read_control_frame::eof",
        ));
    }
    if byte_count < FrameHeader::ENCODED_SIZE {
        return Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_IPC,
            "read_control_frame::short_header",
        ));
    }
    let header = decode_frame_header(&frame_buffer[..FrameHeader::ENCODED_SIZE])?;
    let payload_length = header.payload_length as usize;
    if payload_length > max_payload_length {
        return Err(Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_IPC,
            "read_control_frame::payload_len",
        ));
    }
    let expected_length = FrameHeader::ENCODED_SIZE
        .checked_add(payload_length)
        .ok_or_else(|| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_IPC,
                "read_control_frame::expected_len",
            )
        })?;
    if byte_count == expected_length {
        return Ok(ControlFrame {
            header,
            payload: frame_buffer[FrameHeader::ENCODED_SIZE..expected_length].to_vec(),
        });
    }
    if byte_count != FrameHeader::ENCODED_SIZE {
        return Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_IPC,
            "read_control_frame::frame_len",
        ));
    }

    let mut payload = vec![0_u8; payload_length];
    stream
        .read_exact(&mut payload)
        .map_err(|error| warning_from_io(error, "read_control_frame::payload"))?;
    Ok(ControlFrame { header, payload })
}

/// Reads one control frame and any attached file descriptors
pub fn read_control_frame_with_file_descriptors(
    stream: &mut UnixStream,
    max_payload_length: usize,
    max_file_descriptors: usize,
) -> Result<ControlFrameWithFileDescriptors, Warning> {
    let buffer_length = FrameHeader::ENCODED_SIZE
        .checked_add(max_payload_length)
        .ok_or_else(|| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_IPC,
                "read_control_frame_with_file_descriptors::buffer_len",
            )
        })?;
    let mut frame_buffer = vec![0_u8; buffer_length];
    let received =
        receive_file_descriptors(stream.as_raw_fd(), &mut frame_buffer, max_file_descriptors)?;
    if received.byte_count == 0 {
        return Err(Warning::new(
            WarningCode::Disconnected,
            COMPONENT_IPC,
            "read_control_frame_with_file_descriptors::eof",
        ));
    }
    if received.byte_count < FrameHeader::ENCODED_SIZE {
        return Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_IPC,
            "read_control_frame_with_file_descriptors::short_header",
        ));
    }
    let header = decode_frame_header(&frame_buffer[..FrameHeader::ENCODED_SIZE])?;
    let payload_length = header.payload_length as usize;
    if payload_length > max_payload_length {
        return Err(Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_IPC,
            "read_control_frame_with_file_descriptors::payload_len",
        ));
    }
    let expected_length = FrameHeader::ENCODED_SIZE
        .checked_add(payload_length)
        .ok_or_else(|| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_IPC,
                "read_control_frame_with_file_descriptors::expected_len",
            )
        })?;
    if received.byte_count != expected_length {
        return Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_IPC,
            "read_control_frame_with_file_descriptors::frame_len",
        ));
    }

    Ok(ControlFrameWithFileDescriptors {
        frame: ControlFrame {
            header,
            payload: frame_buffer[FrameHeader::ENCODED_SIZE..expected_length].to_vec(),
        },
        file_descriptors: received.file_descriptors,
    })
}

fn warning_from_io(error: std::io::Error, operation: &'static str) -> Warning {
    let warning_code = match error.kind() {
        std::io::ErrorKind::UnexpectedEof
        | std::io::ErrorKind::ConnectionReset
        | std::io::ErrorKind::BrokenPipe => WarningCode::Disconnected,
        _ => WarningCode::SyscallFailed,
    };
    match error.raw_os_error() {
        Some(errno) => Warning::new(warning_code, COMPONENT_IPC, operation).with_os_errno(errno),
        None => Warning::new(warning_code, COMPONENT_IPC, operation),
    }
}
