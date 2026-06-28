//! Control protocol opcodes
use rbus_core::warning::{Warning, WarningCode};

const COMPONENT_IPC: &str = "rbus-ipc";

/// Control protocol metadata
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum Opcode {
    /// HELLO handshake
    Hello = 1,
    /// REGISTER_PUBLISHER
    RegisterPublisher = 2,
    /// REGISTER_SUBSCRIBER
    RegisterSubscriber = 3,
    /// ATTACH_STREAMS
    AttachStreams = 4,
    /// NOTIFY_STREAM_ADDED
    NotifyStreamAdded = 5,
    /// UNREGISTER
    Unregister = 6,
    /// GET_STATS
    GetStats = 7,
    /// GET_WARNINGS
    GetWarnings = 8,
    /// CLEANUP
    Cleanup = 9,
    /// DOCTOR
    Doctor = 10,
    /// INSPECT
    Inspect = 11,
    /// REREGISTER_PUBLISHER
    ReregisterPublisher = 12,
    /// REREGISTER_SUBSCRIBER
    ReregisterSubscriber = 13,
}

impl TryFrom<u16> for Opcode {
    type Error = Warning;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Hello),
            2 => Ok(Self::RegisterPublisher),
            3 => Ok(Self::RegisterSubscriber),
            4 => Ok(Self::AttachStreams),
            5 => Ok(Self::NotifyStreamAdded),
            6 => Ok(Self::Unregister),
            7 => Ok(Self::GetStats),
            8 => Ok(Self::GetWarnings),
            9 => Ok(Self::Cleanup),
            10 => Ok(Self::Doctor),
            11 => Ok(Self::Inspect),
            12 => Ok(Self::ReregisterPublisher),
            13 => Ok(Self::ReregisterSubscriber),
            _ => Err(Warning::new(
                WarningCode::ProtocolMismatch,
                COMPONENT_IPC,
                "Opcode::try_from",
            )),
        }
    }
}
