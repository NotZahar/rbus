use rbus_ipc::control::{FrameHeader, Opcode};

/// Control response metadata
pub fn handle_hello() -> FrameHeader {
    FrameHeader::new(
        Opcode::Hello,
        rbus_core::constants::CONTROL_PROTOCOL_VERSION,
        0,
    )
}
