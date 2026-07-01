//! HELLO helpers
use rbus_core::constants::CONTROL_PROTOCOL_VERSION;

use super::{FrameHeader, Opcode};

/// Builds the HELLO frame header for the current protocol version
pub const fn hello_frame_header() -> FrameHeader {
    FrameHeader::new(Opcode::Hello, CONTROL_PROTOCOL_VERSION, 0)
}
