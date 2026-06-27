use core::sync::atomic::Ordering;

use rbus_core::stats::StreamCounters;

#[test]
fn stream_counters_snapshot_reads_values() {
    let counters = StreamCounters::new();
    counters.published_messages.store(7, Ordering::Relaxed);
    counters.received_messages.store(5, Ordering::Relaxed);

    let snapshot = counters.snapshot(7, 2, true);
    assert_eq!(snapshot.published_messages, 7);
    assert_eq!(snapshot.received_messages, 5);
    assert_eq!(snapshot.current_sequence, 7);
    assert_eq!(snapshot.subscriber_count, 2);
}
