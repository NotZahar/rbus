//! Topic configuration
use rbus_core::constants::{COMPONENT_CORE, DEFAULT_MAX_MESSAGE_SIZE};
use rbus_core::layout::calculate_segment_layout;
use rbus_core::warning::{Warning, WarningCode};

/// Publisher-side topic configuration
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TopicConfig {
    max_message_size: usize,
    depth: usize,
}

impl TopicConfig {
    /// Creates a topic configuration with maximum payload size and ring depth
    pub const fn new(max_message_size: usize, depth: usize) -> Self {
        Self {
            max_message_size,
            depth,
        }
    }

    /// Returns the maximum accepted payload size in bytes
    pub const fn max_message_size(&self) -> usize {
        self.max_message_size
    }

    /// Returns the ring depth, which must be a power of two
    pub const fn depth(&self) -> usize {
        self.depth
    }

    pub(crate) fn validate_for_publisher(&self) -> Result<(), Warning> {
        if self.max_message_size == 0
            || self.max_message_size > DEFAULT_MAX_MESSAGE_SIZE
            || self.depth == 0
            || !self.depth.is_power_of_two()
        {
            return Err(Warning::new(
                WarningCode::InvalidConfig,
                COMPONENT_CORE,
                "TopicConfig::validate_for_publisher",
            ));
        }
        calculate_segment_layout(self.max_message_size, self.depth)?;
        Ok(())
    }
}
