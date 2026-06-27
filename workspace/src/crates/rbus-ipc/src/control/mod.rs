//! Control protocol metadata

pub mod client;
pub mod codec;
pub mod frame;
pub mod handshake;
pub mod io;
pub mod messages;
pub mod opcode;
pub mod server;

pub use codec::{decode_frame_header, encode_frame_header};
pub use frame::FrameHeader;
pub use opcode::Opcode;
