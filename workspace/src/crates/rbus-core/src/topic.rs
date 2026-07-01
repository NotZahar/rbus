//! Topic name validation and stable topic hashing

use crate::constants::COMPONENT_CORE;
use crate::ids::TopicHash;
use crate::result::RbusResult;
use crate::warning::{Warning, WarningCode};

/// Validates the topic syntax accepted by the runtime
pub fn validate_topic_name(topic_name: &str) -> RbusResult<()> {
    if topic_name.is_empty() || !topic_name.starts_with('/') {
        return Err(Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_CORE,
            "validate_topic_name",
        ));
    }

    if topic_name.len() > 255
        || topic_name
            .as_bytes()
            .windows(2)
            .any(|window| window == b"//")
    {
        return Err(Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_CORE,
            "validate_topic_name",
        ));
    }

    if !topic_name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'-' | b'.'))
    {
        return Err(Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_CORE,
            "validate_topic_name",
        ));
    }

    Ok(())
}

/// Computes the stable FNV-1a topic hash used on the wire
pub fn stable_topic_hash(topic_name: &str) -> TopicHash {
    let mut hash_value = 0xcbf2_9ce4_8422_2325u64;
    for byte in topic_name.as_bytes() {
        hash_value ^= u64::from(*byte);
        hash_value = hash_value.wrapping_mul(0x0000_0100_0000_01b3);
    }
    TopicHash::new(hash_value)
}
