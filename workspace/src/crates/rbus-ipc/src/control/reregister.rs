//! Typed payloads for daemon restart re-registration commands

use rbus_core::warning::{Warning, WarningCode};

const COMPONENT_IPC: &str = "rbus-ipc";
const U64_BYTES: usize = 8;

/// REREGISTER_PUBLISHER request
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReregisterPublisherRequest {
    /// Stable topic hash
    pub topic_hash: u64,
    /// Publisher id from the existing stream
    pub publisher_id: u64,
    /// Stream id from the existing stream
    pub stream_id: u64,
    /// Segment size
    pub segment_size: u64,
    /// Payload slot size
    pub slot_size: u64,
    /// Ring depth
    pub depth: u64,
}

impl ReregisterPublisherRequest {
    /// Encoded payload size
    pub const ENCODED_SIZE: usize = U64_BYTES * 6;

    /// Encodes this request payload in little-endian wire format
    pub fn encode(self) -> [u8; Self::ENCODED_SIZE] {
        let mut output = [0_u8; Self::ENCODED_SIZE];
        write_u64(&mut output, 0, self.topic_hash);
        write_u64(&mut output, 1, self.publisher_id);
        write_u64(&mut output, 2, self.stream_id);
        write_u64(&mut output, 3, self.segment_size);
        write_u64(&mut output, 4, self.slot_size);
        write_u64(&mut output, 5, self.depth);
        output
    }

    /// Decodes this request payload from little-endian wire format
    pub fn decode(input: &[u8]) -> Result<Self, Warning> {
        expect_len(
            input,
            Self::ENCODED_SIZE,
            "ReregisterPublisherRequest::decode",
        )?;
        Ok(Self {
            topic_hash: read_u64(input, 0)?,
            publisher_id: read_u64(input, 1)?,
            stream_id: read_u64(input, 2)?,
            segment_size: read_u64(input, 3)?,
            slot_size: read_u64(input, 4)?,
            depth: read_u64(input, 5)?,
        })
    }
}

/// REREGISTER_PUBLISHER response
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReregisterPublisherResponse {
    /// Daemon registry generation assigned after restart
    pub stream_generation: u64,
    /// Subscribers already registered for this topic after re-registration
    pub subscriber_count: u64,
}

impl ReregisterPublisherResponse {
    /// Encoded payload size
    pub const ENCODED_SIZE: usize = U64_BYTES * 2;

    /// Encodes this response payload in little-endian wire format
    pub fn encode(self) -> [u8; Self::ENCODED_SIZE] {
        let mut output = [0_u8; Self::ENCODED_SIZE];
        write_u64(&mut output, 0, self.stream_generation);
        write_u64(&mut output, 1, self.subscriber_count);
        output
    }

    /// Decodes this response payload from little-endian wire format
    pub fn decode(input: &[u8]) -> Result<Self, Warning> {
        expect_len(
            input,
            Self::ENCODED_SIZE,
            "ReregisterPublisherResponse::decode",
        )?;
        Ok(Self {
            stream_generation: read_u64(input, 0)?,
            subscriber_count: read_u64(input, 1)?,
        })
    }
}

/// REREGISTER_SUBSCRIBER request
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReregisterSubscriberRequest {
    /// Stable topic hash
    pub topic_hash: u64,
}

impl ReregisterSubscriberRequest {
    /// Encoded payload size
    pub const ENCODED_SIZE: usize = U64_BYTES;

    /// Encodes this request payload in little-endian wire format
    pub fn encode(self) -> [u8; Self::ENCODED_SIZE] {
        self.topic_hash.to_le_bytes()
    }

    /// Decodes this request payload from little-endian wire format
    pub fn decode(input: &[u8]) -> Result<Self, Warning> {
        expect_len(
            input,
            Self::ENCODED_SIZE,
            "ReregisterSubscriberRequest::decode",
        )?;
        Ok(Self {
            topic_hash: read_u64(input, 0)?,
        })
    }
}

fn expect_len(input: &[u8], expected: usize, operation: &'static str) -> Result<(), Warning> {
    if input.len() == expected {
        Ok(())
    } else {
        Err(Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_IPC,
            operation,
        ))
    }
}

fn read_u64(input: &[u8], word_index: usize) -> Result<u64, Warning> {
    let start = word_index.checked_mul(U64_BYTES).ok_or_else(|| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_IPC,
            "reregister::read_u64::start",
        )
    })?;
    let end = start.checked_add(U64_BYTES).ok_or_else(|| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_IPC,
            "reregister::read_u64::end",
        )
    })?;
    let bytes = input.get(start..end).ok_or_else(|| {
        Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_IPC,
            "reregister::read_u64::bounds",
        )
    })?;
    Ok(u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn write_u64(output: &mut [u8], word_index: usize, value: u64) {
    let start = word_index * U64_BYTES;
    output[start..start + U64_BYTES].copy_from_slice(&value.to_le_bytes());
}
