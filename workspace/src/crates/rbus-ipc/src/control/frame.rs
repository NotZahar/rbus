//! Control protocol metadata

use super::opcode::Opcode;

/// Fixed-size frame header
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameHeader {
    /// Opcode
    pub opcode: Opcode,
    /// Flags
    pub flags: u16,
    /// Payload metadata
    pub payload_length: u32,
}

impl FrameHeader {
    /// Control protocol metadata
    pub const ENCODED_SIZE: usize = 8;

    /// Control protocol metadata
    pub const fn new(opcode: Opcode, flags: u16, payload_length: u32) -> Self {
        Self {
            opcode,
            flags,
            payload_length,
        }
    }
}
