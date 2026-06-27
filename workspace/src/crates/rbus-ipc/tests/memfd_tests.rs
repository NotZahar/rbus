use rbus_ipc::memfd::create_memfd;

#[test]
fn memfd_create_returns_valid_descriptor() {
    let file_descriptor = create_memfd("rbus-test", 4096).expect("memfd should be created");
    assert!(file_descriptor.as_raw_file_descriptor() >= 0);
}
