use rbus_ipc::eventfd::create_eventfd;

#[test]
fn eventfd_create_returns_descriptor() {
    let file_descriptor = create_eventfd().expect("eventfd should be created");
    assert!(file_descriptor.as_raw_file_descriptor() >= 0);
}
