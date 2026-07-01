use rbus_ipc::control::{FrameHeader, Opcode};

/// Builds the daemon HELLO response header
pub fn handle_hello() -> FrameHeader {
    FrameHeader::new(
        Opcode::Hello,
        rbus_core::constants::CONTROL_PROTOCOL_VERSION,
        0,
    )
}
