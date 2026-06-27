use rbus_ipc::clock::monotonic_timestamp_ns;

#[test]
fn monotonic_timestamp_is_non_zero() {
    let timestamp = monotonic_timestamp_ns().expect("operation should succeed");
    assert!(timestamp > 0);
}
