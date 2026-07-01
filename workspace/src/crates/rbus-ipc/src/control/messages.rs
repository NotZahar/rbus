//! Typed payloads for core control protocol commands

use rbus_core::warning::{Warning, WarningCode};

const COMPONENT_IPC: &str = "rbus-ipc";
const U64_BYTES: usize = 8;
const ATTACH_STREAMS_HEADER_WORDS: usize = 1;
const NOTIFY_STREAM_ADDED_HEADER_WORDS: usize = 2;
const STREAM_ATTACHMENT_WORDS: usize = 6;
const UNREGISTER_ENTITY_PUBLISHER: u64 = 1;
const UNREGISTER_ENTITY_SUBSCRIBER: u64 = 2;
/// Immediate `NOTIFY_STREAM_ADDED` poll without blocking
pub const NOTIFY_STREAM_ADDED_NO_WAIT_NS: u64 = 0;
/// `NOTIFY_STREAM_ADDED` wait with no timeout
pub const NOTIFY_STREAM_ADDED_INFINITE_WAIT_NS: u64 = u64::MAX;

/// REGISTER_PUBLISHER request
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegisterPublisherRequest {
    /// Stable topic hash
    pub topic_hash: u64,
    /// Max message size
    pub max_message_size: u64,
    /// Ring depth
    pub depth: u64,
}

impl RegisterPublisherRequest {
    /// Encoded payload size
    pub const ENCODED_SIZE: usize = U64_BYTES * 3;

    /// Encodes this request payload in little-endian wire format
    pub fn encode(self) -> [u8; Self::ENCODED_SIZE] {
        let mut output = [0_u8; Self::ENCODED_SIZE];
        write_u64(&mut output, 0, self.topic_hash);
        write_u64(&mut output, 1, self.max_message_size);
        write_u64(&mut output, 2, self.depth);
        output
    }

    /// Decodes this request payload from little-endian wire format
    pub fn decode(input: &[u8]) -> Result<Self, Warning> {
        expect_len(
            input,
            Self::ENCODED_SIZE,
            "RegisterPublisherRequest::decode",
        )?;
        Ok(Self {
            topic_hash: read_u64(input, 0)?,
            max_message_size: read_u64(input, 1)?,
            depth: read_u64(input, 2)?,
        })
    }
}

/// REGISTER_PUBLISHER response
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegisterPublisherResponse {
    /// Publisher id
    pub publisher_id: u64,
    /// Stream id
    pub stream_id: u64,
    /// Daemon registry generation where stream was created
    pub stream_generation: u64,
    /// Total byte length of the stream segment
    pub segment_size: u64,
    /// Maximum payload bytes stored in each slot
    pub slot_size: u64,
    /// Ring depth
    pub depth: u64,
    /// Number of subscribers already registered for this topic
    pub subscriber_count: u64,
}

impl RegisterPublisherResponse {
    /// Encoded payload size
    pub const ENCODED_SIZE: usize = U64_BYTES * 7;

    /// Encodes this response payload in little-endian wire format
    pub fn encode(self) -> [u8; Self::ENCODED_SIZE] {
        let mut output = [0_u8; Self::ENCODED_SIZE];
        write_u64(&mut output, 0, self.publisher_id);
        write_u64(&mut output, 1, self.stream_id);
        write_u64(&mut output, 2, self.stream_generation);
        write_u64(&mut output, 3, self.segment_size);
        write_u64(&mut output, 4, self.slot_size);
        write_u64(&mut output, 5, self.depth);
        write_u64(&mut output, 6, self.subscriber_count);
        output
    }

    /// Decodes this response payload from little-endian wire format
    pub fn decode(input: &[u8]) -> Result<Self, Warning> {
        expect_len(
            input,
            Self::ENCODED_SIZE,
            "RegisterPublisherResponse::decode",
        )?;
        Ok(Self {
            publisher_id: read_u64(input, 0)?,
            stream_id: read_u64(input, 1)?,
            stream_generation: read_u64(input, 2)?,
            segment_size: read_u64(input, 3)?,
            slot_size: read_u64(input, 4)?,
            depth: read_u64(input, 5)?,
            subscriber_count: read_u64(input, 6)?,
        })
    }
}

/// REGISTER_SUBSCRIBER request
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegisterSubscriberRequest {
    /// Stable topic hash
    pub topic_hash: u64,
}

impl RegisterSubscriberRequest {
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
            "RegisterSubscriberRequest::decode",
        )?;
        Ok(Self {
            topic_hash: read_u64(input, 0)?,
        })
    }
}

/// REGISTER_SUBSCRIBER response
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegisterSubscriberResponse {
    /// Subscriber id
    pub subscriber_id: u64,
    /// Daemon registry generation at subscription time
    pub subscribed_generation: u64,
}

impl RegisterSubscriberResponse {
    /// Encoded payload size
    pub const ENCODED_SIZE: usize = U64_BYTES * 2;

    /// Encodes this response payload in little-endian wire format
    pub fn encode(self) -> [u8; Self::ENCODED_SIZE] {
        let mut output = [0_u8; Self::ENCODED_SIZE];
        write_u64(&mut output, 0, self.subscriber_id);
        write_u64(&mut output, 1, self.subscribed_generation);
        output
    }

    /// Decodes this response payload from little-endian wire format
    pub fn decode(input: &[u8]) -> Result<Self, Warning> {
        expect_len(
            input,
            Self::ENCODED_SIZE,
            "RegisterSubscriberResponse::decode",
        )?;
        Ok(Self {
            subscriber_id: read_u64(input, 0)?,
            subscribed_generation: read_u64(input, 1)?,
        })
    }
}

/// ATTACH_STREAMS request
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttachStreamsRequest {
    /// Stable topic hash
    pub topic_hash: u64,
}

impl AttachStreamsRequest {
    /// Encoded payload size
    pub const ENCODED_SIZE: usize = U64_BYTES;

    /// Encodes this request payload in little-endian wire format
    pub fn encode(self) -> [u8; Self::ENCODED_SIZE] {
        self.topic_hash.to_le_bytes()
    }

    /// Decodes this request payload from little-endian wire format
    pub fn decode(input: &[u8]) -> Result<Self, Warning> {
        expect_len(input, Self::ENCODED_SIZE, "AttachStreamsRequest::decode")?;
        Ok(Self {
            topic_hash: read_u64(input, 0)?,
        })
    }
}

/// Stream descriptor metadata sent together with a memfd
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamAttachment {
    /// Publisher id
    pub publisher_id: u64,
    /// Stream id
    pub stream_id: u64,
    /// Daemon registry generation where stream was created
    pub stream_generation: u64,
    /// Total byte length of the stream segment
    pub segment_size: u64,
    /// Maximum payload bytes stored in each slot
    pub slot_size: u64,
    /// Ring depth
    pub depth: u64,
}

impl StreamAttachment {
    const ENCODED_SIZE: usize = U64_BYTES * STREAM_ATTACHMENT_WORDS;

    fn encode_into(self, output: &mut [u8], start_word: usize) {
        write_u64(output, start_word, self.publisher_id);
        write_u64(output, start_word + 1, self.stream_id);
        write_u64(output, start_word + 2, self.stream_generation);
        write_u64(output, start_word + 3, self.segment_size);
        write_u64(output, start_word + 4, self.slot_size);
        write_u64(output, start_word + 5, self.depth);
    }

    fn decode_from(input: &[u8], start_word: usize) -> Result<Self, Warning> {
        Ok(Self {
            publisher_id: read_u64(input, start_word)?,
            stream_id: read_u64(input, start_word + 1)?,
            stream_generation: read_u64(input, start_word + 2)?,
            segment_size: read_u64(input, start_word + 3)?,
            slot_size: read_u64(input, start_word + 4)?,
            depth: read_u64(input, start_word + 5)?,
        })
    }
}

/// ATTACH_STREAMS response
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachStreamsResponse {
    /// Streams matching the requested topic
    pub streams: Vec<StreamAttachment>,
}

impl AttachStreamsResponse {
    /// Encodes this variable-length response payload
    pub fn encode(&self) -> Result<Vec<u8>, Warning> {
        let payload_length = U64_BYTES
            .checked_add(
                self.streams
                    .len()
                    .checked_mul(StreamAttachment::ENCODED_SIZE)
                    .ok_or_else(|| {
                        Warning::new(
                            WarningCode::ResourceLimit,
                            COMPONENT_IPC,
                            "AttachStreamsResponse::encode::streams_len",
                        )
                    })?,
            )
            .ok_or_else(|| {
                Warning::new(
                    WarningCode::ResourceLimit,
                    COMPONENT_IPC,
                    "AttachStreamsResponse::encode::payload_len",
                )
            })?;
        let mut output = vec![0_u8; payload_length];
        write_u64(&mut output, 0, self.streams.len() as u64);
        for (stream_index, stream) in self.streams.iter().copied().enumerate() {
            stream.encode_into(
                &mut output,
                ATTACH_STREAMS_HEADER_WORDS + stream_index * STREAM_ATTACHMENT_WORDS,
            );
        }
        Ok(output)
    }

    /// Decodes this variable-length response payload
    pub fn decode(input: &[u8]) -> Result<Self, Warning> {
        if input.len() < U64_BYTES {
            return Err(Warning::new(
                WarningCode::ProtocolMismatch,
                COMPONENT_IPC,
                "AttachStreamsResponse::decode::count",
            ));
        }
        let stream_count = usize::try_from(read_u64(input, 0)?).map_err(|_| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_IPC,
                "AttachStreamsResponse::decode::count_usize",
            )
        })?;
        let expected_length = U64_BYTES
            .checked_add(
                stream_count
                    .checked_mul(StreamAttachment::ENCODED_SIZE)
                    .ok_or_else(|| {
                        Warning::new(
                            WarningCode::ResourceLimit,
                            COMPONENT_IPC,
                            "AttachStreamsResponse::decode::streams_len",
                        )
                    })?,
            )
            .ok_or_else(|| {
                Warning::new(
                    WarningCode::ResourceLimit,
                    COMPONENT_IPC,
                    "AttachStreamsResponse::decode::payload_len",
                )
            })?;
        expect_len(input, expected_length, "AttachStreamsResponse::decode::len")?;
        let mut streams = Vec::with_capacity(stream_count);
        for stream_index in 0..stream_count {
            streams.push(StreamAttachment::decode_from(
                input,
                ATTACH_STREAMS_HEADER_WORDS + stream_index * STREAM_ATTACHMENT_WORDS,
            )?);
        }
        Ok(Self { streams })
    }
}

/// NOTIFY_STREAM_ADDED request
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotifyStreamAddedRequest {
    /// Stable topic hash
    pub topic_hash: u64,
    /// Daemon-issued subscriber id
    pub subscriber_id: u64,
    /// Registry generation the subscriber has already observed
    pub since_generation: u64,
    /// Maximum time to wait for newer streams, in nanoseconds
    pub wait_timeout_ns: u64,
}

impl NotifyStreamAddedRequest {
    /// Encoded payload size
    pub const ENCODED_SIZE: usize = U64_BYTES * 4;

    /// Encodes this request payload in little-endian wire format
    pub fn encode(self) -> [u8; Self::ENCODED_SIZE] {
        let mut output = [0_u8; Self::ENCODED_SIZE];
        write_u64(&mut output, 0, self.topic_hash);
        write_u64(&mut output, 1, self.subscriber_id);
        write_u64(&mut output, 2, self.since_generation);
        write_u64(&mut output, 3, self.wait_timeout_ns);
        output
    }

    /// Decodes this request payload from little-endian wire format
    pub fn decode(input: &[u8]) -> Result<Self, Warning> {
        expect_len(
            input,
            Self::ENCODED_SIZE,
            "NotifyStreamAddedRequest::decode",
        )?;
        Ok(Self {
            topic_hash: read_u64(input, 0)?,
            subscriber_id: read_u64(input, 1)?,
            since_generation: read_u64(input, 2)?,
            wait_timeout_ns: read_u64(input, 3)?,
        })
    }
}

/// NOTIFY_STREAM_ADDED response
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotifyStreamAddedResponse {
    /// Latest registry generation known to the daemon
    pub latest_generation: u64,
    /// Streams created after the requested generation
    pub streams: Vec<StreamAttachment>,
}

impl NotifyStreamAddedResponse {
    /// Encodes this variable-length response payload
    pub fn encode(&self) -> Result<Vec<u8>, Warning> {
        let payload_length = U64_BYTES
            .checked_mul(NOTIFY_STREAM_ADDED_HEADER_WORDS)
            .and_then(|header_length| {
                self.streams
                    .len()
                    .checked_mul(StreamAttachment::ENCODED_SIZE)
                    .and_then(|streams_length| header_length.checked_add(streams_length))
            })
            .ok_or_else(|| {
                Warning::new(
                    WarningCode::ResourceLimit,
                    COMPONENT_IPC,
                    "NotifyStreamAddedResponse::encode::payload_len",
                )
            })?;
        let mut output = vec![0_u8; payload_length];
        write_u64(&mut output, 0, self.latest_generation);
        write_u64(&mut output, 1, self.streams.len() as u64);
        for (stream_index, stream) in self.streams.iter().copied().enumerate() {
            stream.encode_into(
                &mut output,
                NOTIFY_STREAM_ADDED_HEADER_WORDS + stream_index * STREAM_ATTACHMENT_WORDS,
            );
        }
        Ok(output)
    }

    /// Decodes this variable-length response payload
    pub fn decode(input: &[u8]) -> Result<Self, Warning> {
        if input.len() < U64_BYTES * NOTIFY_STREAM_ADDED_HEADER_WORDS {
            return Err(Warning::new(
                WarningCode::ProtocolMismatch,
                COMPONENT_IPC,
                "NotifyStreamAddedResponse::decode::header",
            ));
        }
        let stream_count = usize::try_from(read_u64(input, 1)?).map_err(|_| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_IPC,
                "NotifyStreamAddedResponse::decode::count_usize",
            )
        })?;
        let expected_length = U64_BYTES
            .checked_mul(NOTIFY_STREAM_ADDED_HEADER_WORDS)
            .and_then(|header_length| {
                stream_count
                    .checked_mul(StreamAttachment::ENCODED_SIZE)
                    .and_then(|streams_length| header_length.checked_add(streams_length))
            })
            .ok_or_else(|| {
                Warning::new(
                    WarningCode::ResourceLimit,
                    COMPONENT_IPC,
                    "NotifyStreamAddedResponse::decode::payload_len",
                )
            })?;
        expect_len(
            input,
            expected_length,
            "NotifyStreamAddedResponse::decode::len",
        )?;
        let mut streams = Vec::with_capacity(stream_count);
        for stream_index in 0..stream_count {
            streams.push(StreamAttachment::decode_from(
                input,
                NOTIFY_STREAM_ADDED_HEADER_WORDS + stream_index * STREAM_ATTACHMENT_WORDS,
            )?);
        }
        Ok(Self {
            latest_generation: read_u64(input, 0)?,
            streams,
        })
    }
}

/// Entity kind targeted by an `UNREGISTER` request
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnregisterTargetKind {
    /// Publisher session id
    Publisher,
    /// Subscriber id
    Subscriber,
}

impl UnregisterTargetKind {
    const fn as_u64(self) -> u64 {
        match self {
            Self::Publisher => UNREGISTER_ENTITY_PUBLISHER,
            Self::Subscriber => UNREGISTER_ENTITY_SUBSCRIBER,
        }
    }
}

impl TryFrom<u64> for UnregisterTargetKind {
    type Error = Warning;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value {
            UNREGISTER_ENTITY_PUBLISHER => Ok(Self::Publisher),
            UNREGISTER_ENTITY_SUBSCRIBER => Ok(Self::Subscriber),
            _ => Err(Warning::new(
                WarningCode::ProtocolMismatch,
                COMPONENT_IPC,
                "UnregisterTargetKind::try_from",
            )),
        }
    }
}

/// UNREGISTER request
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnregisterRequest {
    /// Target kind
    pub target_kind: UnregisterTargetKind,
    /// Publisher or subscriber id selected by `target_kind`
    pub target_id: u64,
}

impl UnregisterRequest {
    /// Encoded payload size
    pub const ENCODED_SIZE: usize = U64_BYTES * 2;

    /// Encodes this request payload in little-endian wire format
    pub fn encode(self) -> [u8; Self::ENCODED_SIZE] {
        let mut output = [0_u8; Self::ENCODED_SIZE];
        write_u64(&mut output, 0, self.target_kind.as_u64());
        write_u64(&mut output, 1, self.target_id);
        output
    }

    /// Decodes this request payload from little-endian wire format
    pub fn decode(input: &[u8]) -> Result<Self, Warning> {
        expect_len(input, Self::ENCODED_SIZE, "UnregisterRequest::decode")?;
        Ok(Self {
            target_kind: UnregisterTargetKind::try_from(read_u64(input, 0)?)?,
            target_id: read_u64(input, 1)?,
        })
    }
}

/// UNREGISTER response
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnregisterResponse {
    /// Number of registry entries removed
    pub removed_count: u64,
    /// Registry generation after unregister handling
    pub registry_generation: u64,
}

impl UnregisterResponse {
    /// Encoded payload size
    pub const ENCODED_SIZE: usize = U64_BYTES * 2;

    /// Encodes this response payload in little-endian wire format
    pub fn encode(self) -> [u8; Self::ENCODED_SIZE] {
        let mut output = [0_u8; Self::ENCODED_SIZE];
        write_u64(&mut output, 0, self.removed_count);
        write_u64(&mut output, 1, self.registry_generation);
        output
    }

    /// Decodes this response payload from little-endian wire format
    pub fn decode(input: &[u8]) -> Result<Self, Warning> {
        expect_len(input, Self::ENCODED_SIZE, "UnregisterResponse::decode")?;
        Ok(Self {
            removed_count: read_u64(input, 0)?,
            registry_generation: read_u64(input, 1)?,
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
            "messages::read_u64::start",
        )
    })?;
    let end = start.checked_add(U64_BYTES).ok_or_else(|| {
        Warning::new(
            WarningCode::ResourceLimit,
            COMPONENT_IPC,
            "messages::read_u64::end",
        )
    })?;
    let bytes = input.get(start..end).ok_or_else(|| {
        Warning::new(
            WarningCode::ProtocolMismatch,
            COMPONENT_IPC,
            "messages::read_u64::bounds",
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
