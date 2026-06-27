//! Encode/decode control frames
use rbus_core::warning::{Warning, WarningCode};

use super::frame::FrameHeader;
use super::opcode::Opcode;

const COMPONENT_IPC: &str = "rbus-ipc";

/// Control protocol metadata
pub fn encode_frame_header(
    frame_header: FrameHeader,
    output_buffer: &mut [u8],
) -> Result<(), Warning> {
    if output_buffer.len() < FrameHeader::ENCODED_SIZE {
        return Err(Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_IPC,
            "encode_frame_header",
        ));
    }
    output_buffer[0..2].copy_from_slice(&(frame_header.opcode as u16).to_le_bytes());
    output_buffer[2..4].copy_from_slice(&frame_header.flags.to_le_bytes());
    output_buffer[4..8].copy_from_slice(&frame_header.payload_length.to_le_bytes());
    Ok(())
}

/// Control protocol metadata
pub fn decode_frame_header(input_buffer: &[u8]) -> Result<FrameHeader, Warning> {
    if input_buffer.len() < FrameHeader::ENCODED_SIZE {
        return Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_IPC,
            "decode_frame_header",
        ));
    }
    let opcode_value = u16::from_le_bytes([input_buffer[0], input_buffer[1]]);
    let flags = u16::from_le_bytes([input_buffer[2], input_buffer[3]]);
    let payload_length = u32::from_le_bytes([
        input_buffer[4],
        input_buffer[5],
        input_buffer[6],
        input_buffer[7],
    ]);
    Ok(FrameHeader::new(
        Opcode::try_from(opcode_value)?,
        flags,
        payload_length,
    ))
}
