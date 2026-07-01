//! Control server helpers
use super::FrameHeader;

/// Header decoded by the control server before dispatch
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodedControlFrame {
    /// Parsed frame header
    pub header: FrameHeader,
}
