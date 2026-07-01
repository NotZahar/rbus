//! Fixed-size control protocol frame header

use super::opcode::Opcode;

/// Fixed-size frame header
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameHeader {
    /// Opcode
    pub opcode: Opcode,
    /// Flags
    pub flags: u16,
    /// Payload byte length following the frame header
    pub payload_length: u32,
}

impl FrameHeader {
    /// Encoded byte length of a frame header
    pub const ENCODED_SIZE: usize = 8;

    /// Creates a frame header with an opcode, flags and payload length
    pub const fn new(opcode: Opcode, flags: u16, payload_length: u32) -> Self {
        Self {
            opcode,
            flags,
            payload_length,
        }
    }
}
