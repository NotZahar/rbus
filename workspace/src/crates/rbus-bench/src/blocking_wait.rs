use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use rbus_core::warning::{Warning, WarningCode};
use rbus_ipc::futex::{FutexWaitStatus, futex_wait_shared};
use rbus_ipc::topic_wait::{TopicWaitSegment, create_topic_wait_segment};

const BENCH_TOPIC_HASH: u64 = 0x7262_7573_7761_6974;
const BENCH_WAITER_MASK: u64 = 1;
const BARRIER_PARTICIPANTS: usize = 2;
const WAIT_TIMEOUT: Duration = Duration::from_secs(1);

/// Measures wake latency for the blocking topic wait path
pub fn blocking_wait_wake_latency_ns() -> Result<u64, Warning> {
    let file_descriptor = create_topic_wait_segment(BENCH_TOPIC_HASH)?;
    let topic_wait = Arc::new(TopicWaitSegment::map(&file_descriptor)?);
    let barrier = Arc::new(Barrier::new(BARRIER_PARTICIPANTS));
    let waiter_topic_wait = Arc::clone(&topic_wait);
    let waiter_barrier = Arc::clone(&barrier);
    let waiter = std::thread::Builder::new()
        .spawn(move || wait_for_topic_wake(waiter_topic_wait, waiter_barrier))
        .map_err(|error| match error.raw_os_error() {
            Some(errno) => Warning::new(
                WarningCode::ResourceLimit,
                "rbus-bench",
                "blocking_wait_wake_latency_ns::spawn",
            )
            .with_os_errno(errno),
            None => Warning::new(
                WarningCode::ResourceLimit,
                "rbus-bench",
                "blocking_wait_wake_latency_ns::spawn",
            ),
        })?;

    barrier.wait();
    let started_at = Instant::now();
    topic_wait.wake_waiters()?;
    waiter.join().map_err(|_| {
        Warning::new(
            WarningCode::InternalInvariantWarning,
            "rbus-bench",
            "blocking_wait_wake_latency_ns::join",
        )
    })??;
    u64::try_from(started_at.elapsed().as_nanos()).map_err(|_| {
        Warning::new(
            WarningCode::ResourceLimit,
            "rbus-bench",
            "blocking_wait_wake_latency_ns::elapsed",
        )
    })
}

fn wait_for_topic_wake(
    topic_wait: Arc<TopicWaitSegment>,
    barrier: Arc<Barrier>,
) -> Result<(), Warning> {
    let expected_epoch = topic_wait.wake_epoch();
    let _waiter_guard = TopicWaiterGuard::new(&topic_wait);
    barrier.wait();
    match futex_wait_shared(
        topic_wait.wake_epoch_word(),
        expected_epoch,
        Some(WAIT_TIMEOUT),
    )? {
        FutexWaitStatus::Woken | FutexWaitStatus::ValueChanged => Ok(()),
        FutexWaitStatus::TimedOut => Err(Warning::new(
            WarningCode::SyscallFailed,
            "rbus-bench",
            "wait_for_topic_wake::timeout",
        )),
    }
}

struct TopicWaiterGuard<'a> {
    topic_wait: &'a TopicWaitSegment,
}

impl<'a> TopicWaiterGuard<'a> {
    fn new(topic_wait: &'a TopicWaitSegment) -> Self {
        topic_wait.add_waiter(BENCH_WAITER_MASK);
        Self { topic_wait }
    }
}

impl Drop for TopicWaiterGuard<'_> {
    fn drop(&mut self) {
        self.topic_wait.remove_waiter(BENCH_WAITER_MASK);
    }
}
