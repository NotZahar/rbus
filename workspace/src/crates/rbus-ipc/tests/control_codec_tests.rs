use rbus_core::constants::CONTROL_PROTOCOL_VERSION;
use rbus_ipc::control::client::hello_request_header;
use rbus_ipc::control::reregister::{
    ReregisterPublisherRequest, ReregisterPublisherResponse, ReregisterSubscriberRequest,
};
use rbus_ipc::control::{FrameHeader, Opcode, decode_frame_header, encode_frame_header};

const TEST_TOPIC_HASH: u64 = 11;
const TEST_PUBLISHER_ID: u64 = 12;
const TEST_STREAM_ID: u64 = 13;
const TEST_SEGMENT_SIZE: u64 = 4096;
const TEST_SLOT_SIZE: u64 = 64;
const TEST_DEPTH: u64 = 8;
const TEST_STREAM_GENERATION: u64 = 14;
const TEST_SUBSCRIBER_COUNT: u64 = 15;

#[test]
fn control_frame_header_roundtrip() {
    let frame_header = FrameHeader::new(Opcode::RegisterPublisher, 7, 128);
    let mut buffer = [0u8; FrameHeader::ENCODED_SIZE];
    encode_frame_header(frame_header, &mut buffer).expect("header operation should succeed");
    let decoded_frame_header =
        decode_frame_header(&buffer).expect("header operation should succeed");
    assert_eq!(decoded_frame_header, frame_header);
}

#[test]
fn control_client_hello_header_uses_protocol_version() {
    let header = hello_request_header();

    assert_eq!(header.opcode, Opcode::Hello);
    assert_eq!(header.flags, CONTROL_PROTOCOL_VERSION);
    assert_eq!(header.payload_length, 0);
}

#[test]
fn reregister_publisher_request_roundtrip() {
    let request = ReregisterPublisherRequest {
        topic_hash: TEST_TOPIC_HASH,
        publisher_id: TEST_PUBLISHER_ID,
        stream_id: TEST_STREAM_ID,
        segment_size: TEST_SEGMENT_SIZE,
        slot_size: TEST_SLOT_SIZE,
        depth: TEST_DEPTH,
    };

    let decoded = ReregisterPublisherRequest::decode(&request.encode())
        .expect("control request should decode");

    assert_eq!(decoded, request);
}

#[test]
fn reregister_publisher_response_roundtrip() {
    let response = ReregisterPublisherResponse {
        stream_generation: TEST_STREAM_GENERATION,
        subscriber_count: TEST_SUBSCRIBER_COUNT,
    };

    let decoded = ReregisterPublisherResponse::decode(&response.encode())
        .expect("control response should decode");

    assert_eq!(decoded, response);
}

#[test]
fn reregister_subscriber_request_roundtrip() {
    let request = ReregisterSubscriberRequest {
        topic_hash: TEST_TOPIC_HASH,
    };

    let decoded = ReregisterSubscriberRequest::decode(&request.encode())
        .expect("control request should decode");

    assert_eq!(decoded, request);
}
