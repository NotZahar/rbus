use rbus_ipc::topic_wait::{TopicWaitSegment, create_topic_wait_segment};

const TEST_TOPIC_HASH: u64 = 0x7262_7573_7761_6974;
const FIRST_WAITER_MASK: u64 = 1;
const INITIAL_EPOCH: u32 = 0;
const FIRST_EPOCH: u32 = 1;

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
