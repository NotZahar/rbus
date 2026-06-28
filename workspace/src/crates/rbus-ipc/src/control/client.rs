//! Control protocol metadata

use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::Path;

use rbus_core::warning::{Warning, WarningCode};

use crate::unix_socket::connect_unix_stream;

use super::FrameHeader;
use super::handshake::hello_frame_header;
use super::io::{
    ControlFrame, ControlFrameWithFileDescriptors, read_control_frame,
    read_control_frame_with_file_descriptors, write_control_frame_with_file_descriptors,
};
use super::opcode::Opcode;

/// Daemon runtime metadata
pub struct ControlClient {
    stream: UnixStream,
}

impl ControlClient {
    /// Daemon runtime metadata
    pub fn connect(socket_path: &Path) -> Result<Self, Warning> {
        Ok(Self {
            stream: connect_unix_stream(socket_path)?,
        })
    }

    /// Linux IPC metadata
    pub fn as_raw_file_descriptor(&self) -> RawFd {
        self.stream.as_raw_fd()
    }

    /// Control protocol metadata
    pub fn request_frame(
        &mut self,
        opcode: Opcode,
        flags: u16,
        payload: &[u8],
        max_response_payload_length: usize,
    ) -> Result<ControlFrame, Warning> {
        let payload_length = control_payload_length(payload.len(), "ControlClient::request_frame")?;
        write_control_frame_with_file_descriptors(
            &mut self.stream,
            FrameHeader::new(opcode, flags, payload_length),
            payload,
            &[],
        )?;
        read_control_frame(&mut self.stream, max_response_payload_length)
    }

    /// Control protocol metadata
    pub fn request_frame_with_file_descriptors(
        &mut self,
        opcode: Opcode,
        flags: u16,
        payload: &[u8],
        max_response_payload_length: usize,
        max_file_descriptors: usize,
    ) -> Result<ControlFrameWithFileDescriptors, Warning> {
        let payload_length = control_payload_length(
            payload.len(),
            "ControlClient::request_frame_with_file_descriptors",
        )?;
        write_control_frame_with_file_descriptors(
            &mut self.stream,
            FrameHeader::new(opcode, flags, payload_length),
            payload,
            &[],
        )?;
        read_control_frame_with_file_descriptors(
            &mut self.stream,
            max_response_payload_length,
            max_file_descriptors,
        )
    }

    /// Control protocol metadata
    pub fn request_frame_with_sent_file_descriptors(
        &mut self,
        opcode: Opcode,
        flags: u16,
        payload: &[u8],
        file_descriptors: &[RawFd],
        max_response_payload_length: usize,
        max_response_file_descriptors: usize,
    ) -> Result<ControlFrameWithFileDescriptors, Warning> {
        let payload_length = control_payload_length(
            payload.len(),
            "ControlClient::request_frame_with_sent_file_descriptors",
        )?;
        write_control_frame_with_file_descriptors(
            &mut self.stream,
            FrameHeader::new(opcode, flags, payload_length),
            payload,
            file_descriptors,
        )?;
        read_control_frame_with_file_descriptors(
            &mut self.stream,
            max_response_payload_length,
            max_response_file_descriptors,
        )
    }
}

/// Control request metadata
pub const fn hello_request_header() -> FrameHeader {
    hello_frame_header()
}

/// Daemon runtime metadata
pub fn request_control_frame(
    socket_path: &Path,
    opcode: Opcode,
    flags: u16,
    payload: &[u8],
    max_response_payload_length: usize,
) -> Result<ControlFrame, Warning> {
    ControlClient::connect(socket_path)?.request_frame(
        opcode,
        flags,
        payload,
        max_response_payload_length,
    )
}

/// Control protocol metadata
pub fn request_control_frame_with_file_descriptors(
    socket_path: &Path,
    opcode: Opcode,
    flags: u16,
    payload: &[u8],
    max_response_payload_length: usize,
    max_file_descriptors: usize,
) -> Result<ControlFrameWithFileDescriptors, Warning> {
    ControlClient::connect(socket_path)?.request_frame_with_file_descriptors(
        opcode,
        flags,
        payload,
        max_response_payload_length,
        max_file_descriptors,
    )
}

fn control_payload_length(length: usize, operation: &'static str) -> Result<u32, Warning> {
    u32::try_from(length)
        .map_err(|_| Warning::new(WarningCode::ResourceLimit, "rbus-ipc", operation))
}
