use rbus_core::constants::CONTROL_PROTOCOL_VERSION;
use rbus_ipc::control::client::hello_request_header;
use rbus_ipc::control::{FrameHeader, Opcode, decode_frame_header, encode_frame_header};

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
