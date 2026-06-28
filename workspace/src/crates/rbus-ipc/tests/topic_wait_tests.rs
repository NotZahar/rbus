use rbus_core::constants::{TOPIC_WAIT_ABI_VERSION, TOPIC_WAIT_MAGIC};
use rbus_core::warning::WarningCode;
use rbus_ipc::memfd::{create_memfd, seal_file_descriptor_size};
use rbus_ipc::topic_wait::{TopicWaitHeader, TopicWaitSegment, create_topic_wait_segment};

const TEST_TOPIC_HASH: u64 = 0x7262_7573_7761_6974;
const FIRST_WAITER_MASK: u64 = 1;
const INITIAL_EPOCH: u32 = 0;
const FIRST_EPOCH: u32 = 1;
const WRONG_TOPIC_WAIT_NAME: &str = "rbus-topic-wait-wrong-abi";

#[test]
fn topic_wait_segment_header_exposes_current_abi() {
    let file_descriptor =
        create_topic_wait_segment(TEST_TOPIC_HASH).expect("topic wait segment should be created");
    let topic_wait =
        TopicWaitSegment::map(&file_descriptor).expect("topic wait segment should map");

    assert_eq!(topic_wait.magic(), TOPIC_WAIT_MAGIC);
    assert_eq!(topic_wait.abi_version(), TOPIC_WAIT_ABI_VERSION);
}

#[test]
fn topic_wait_segment_rejects_wrong_abi_header() {
    let file_descriptor = create_memfd(
        WRONG_TOPIC_WAIT_NAME,
        core::mem::size_of::<TopicWaitHeader>(),
    )
    .expect("topic wait memfd should be created");
    seal_file_descriptor_size(&file_descriptor).expect("topic wait memfd should be sealed");

    let warning = match TopicWaitSegment::map(&file_descriptor) {
        Ok(_) => panic!("topic wait segment should reject a wrong ABI header"),
        Err(warning) => warning,
    };

    assert_eq!(warning.code(), WarningCode::SegmentAbiMismatch);
}

#[test]
fn wake_waiters_does_not_update_epoch_without_waiters() {
    let file_descriptor =
        create_topic_wait_segment(TEST_TOPIC_HASH).expect("topic wait segment should be created");
    let topic_wait =
        TopicWaitSegment::map(&file_descriptor).expect("topic wait segment should map");

    topic_wait
        .wake_waiters()
        .expect("wake operation should succeed");

    assert_eq!(topic_wait.wake_epoch(), INITIAL_EPOCH);
}

#[test]
fn wake_waiters_updates_epoch_with_waiter_mask() {
    let file_descriptor =
        create_topic_wait_segment(TEST_TOPIC_HASH).expect("topic wait segment should be created");
    let topic_wait =
        TopicWaitSegment::map(&file_descriptor).expect("topic wait segment should map");

    topic_wait.add_waiter(FIRST_WAITER_MASK);
    topic_wait
        .wake_waiters()
        .expect("wake operation should succeed");

    assert_eq!(topic_wait.wake_epoch(), FIRST_EPOCH);
}
