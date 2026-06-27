//! Control server helpers
use super::FrameHeader;

/// Internal documentation
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodedControlFrame {
    /// Control protocol metadata
    pub header: FrameHeader,
}
