//! Subscriber API
use core::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rbus_core::ids::{StreamId, SubscriberId, TopicHash};
use rbus_core::ring::pin::reader_bit_mask;
use rbus_core::sequence::Sequence;
use rbus_core::stats::Stats;
use rbus_core::warning::{Warning, WarningCode};
use rbus_ipc::control::client::ControlClient;
use rbus_ipc::futex::{FutexWaitStatus, futex_wait_shared};
use rbus_ipc::topic_wait::TopicWaitSegment;

use crate::control::{AttachedStream, attach_streams, notify_stream_added, wait_stream_added};
use crate::message_ref::MessageRef;
use crate::stream::{ReadAttempt, StreamInner};
use crate::wait::ReceiveWaitStatus;

const COMPONENT_RBUS: &str = "rbus";

/// Subscriber runtime metadata
pub struct Subscriber {
    socket_path: PathBuf,
    topic_hash: TopicHash,
    subscriber_id: SubscriberId,
    subscribed_generation: u64,
    last_seen_generation: Cell<u64>,
    last_checked_stream_epoch: Cell<u32>,
    streams: RefCell<Vec<SubscriberStream>>,
    next_stream_index: Cell<usize>,
    topic_wait: Arc<TopicWaitSegment>,
    _control_client: ControlClient,
}

impl Subscriber {
    pub(crate) fn new(
        socket_path: PathBuf,
        topic_hash: TopicHash,
        subscriber_id: SubscriberId,
        subscribed_generation: u64,
        topic_wait: Arc<TopicWaitSegment>,
        control_client: ControlClient,
    ) -> Self {
        Self {
            socket_path,
            topic_hash,
            subscriber_id,
            subscribed_generation,
            last_seen_generation: Cell::new(subscribed_generation),
            last_checked_stream_epoch: Cell::new(topic_wait.stream_epoch()),
            streams: RefCell::new(Vec::new()),
            next_stream_index: Cell::new(0),
            topic_wait,
            _control_client: control_client,
        }
    }

    pub(crate) fn attach_existing_streams(&self) -> Result<(), Warning> {
        let attached_streams = attach_streams(&self.socket_path, self.topic_hash)?;
        let max_generation = self.attach_streams_to_set(attached_streams)?;
        self.last_seen_generation
            .set(max_generation.max(self.last_seen_generation.get()));
        Ok(())
    }

    pub(crate) fn refresh_streams(&self) -> Result<(), Warning> {
        let notification = notify_stream_added(
            &self.socket_path,
            self.topic_hash,
            self.subscriber_id,
            self.last_seen_generation.get(),
        )?;
        self.apply_stream_notification(notification)?;
        Ok(())
    }

    fn apply_stream_notification(
        &self,
        notification: crate::control::StreamNotification,
    ) -> Result<bool, Warning> {
        let has_streams = !notification.streams.is_empty();
        let max_generation = self.attach_streams_to_set(notification.streams)?;
        self.last_seen_generation.set(
            notification
                .latest_generation
                .max(max_generation)
                .max(self.last_seen_generation.get()),
        );
        Ok(has_streams)
    }

    fn attach_streams_to_set(&self, attached_streams: Vec<AttachedStream>) -> Result<u64, Warning> {
        let mut streams = self.streams.try_borrow_mut().map_err(|_| {
            Warning::new(
                WarningCode::InternalInvariantWarning,
                COMPONENT_RBUS,
                "Subscriber::attach_streams_to_set::borrow",
            )
        })?;
        let mut max_generation = self.last_seen_generation.get();
        for attached_stream in attached_streams {
            if !streams.iter().any(|existing_stream| {
                existing_stream.stream_id == attached_stream.metadata.stream_id
            }) {
                max_generation = max_generation.max(attached_stream.stream_generation);
                let stream = StreamInner::attach(
                    attached_stream.metadata,
                    attached_stream.stream_generation,
                    &attached_stream.file_descriptor,
                    0,
                )?;
                let cursor = if stream.created_generation() > self.subscribed_generation {
                    Sequence::new(0)
                } else {
                    stream.current_write_sequence()
                };
                streams.push(SubscriberStream {
                    stream_id: stream.stream_id(),
                    cursor,
                    stream,
                });
            }
        }
        Ok(max_generation)
    }

    /// Internal documentation
    pub fn try_recv(&self) -> Result<Option<MessageRef<'_>>, Warning> {
        let streams_empty = {
            let streams = self.streams.try_borrow().map_err(|_| {
                Warning::new(
                    WarningCode::InternalInvariantWarning,
                    COMPONENT_RBUS,
                    "Subscriber::try_recv::initial_borrow",
                )
            })?;
            streams.is_empty()
        };
        if streams_empty {
            let observed_stream_epoch = self.topic_wait.stream_epoch();
            self.refresh_streams()?;
            self.last_checked_stream_epoch.set(observed_stream_epoch);
        }
        let observed_stream_epoch = self.topic_wait.stream_epoch();
        if observed_stream_epoch != self.last_checked_stream_epoch.get() {
            self.refresh_streams()?;
            self.last_checked_stream_epoch.set(observed_stream_epoch);
        }
        self.try_recv_from_attached_streams()
    }

    fn try_recv_from_attached_streams(&self) -> Result<Option<MessageRef<'_>>, Warning> {
        let mut streams = self.streams.try_borrow_mut().map_err(|_| {
            Warning::new(
                WarningCode::InternalInvariantWarning,
                COMPONENT_RBUS,
                "Subscriber::try_recv_from_attached_streams::borrow",
            )
        })?;
        if streams.is_empty() {
            return Ok(None);
        }

        let start_index = self.next_stream_index.get() % streams.len();
        for offset in 0..streams.len() {
            let stream_index = (start_index + offset) % streams.len();
            let subscriber_stream = &mut streams[stream_index];
            match subscriber_stream
                .stream
                .try_read_message(subscriber_stream.cursor, self.subscriber_id)?
            {
                ReadAttempt::Empty => {}
                ReadAttempt::Message(read_reservation) => {
                    subscriber_stream.cursor = subscriber_stream.cursor.next();
                    self.next_stream_index
                        .set((stream_index + 1) % streams.len());
                    return Ok(Some(MessageRef::from_reservation(read_reservation)));
                }
                ReadAttempt::Skipped {
                    next_sequence,
                    warning,
                } => {
                    subscriber_stream.cursor = next_sequence;
                    self.next_stream_index.set(stream_index);
                    return Err(warning);
                }
            }
        }
        Ok(None)
    }

    /// Warning model metadata
    pub fn recv(&self) -> Result<MessageRef<'_>, Warning> {
        loop {
            if let Some(message) = self.try_recv()? {
                return Ok(message);
            }
            let _wait_status = self.wait_for_message_or_stream(None)?;
        }
    }

    /// Warning model metadata
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Option<MessageRef<'_>>, Warning> {
        let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
            Warning::new(
                WarningCode::ResourceLimit,
                COMPONENT_RBUS,
                "Subscriber::recv_timeout::deadline",
            )
        })?;
        loop {
            if let Some(message) = self.try_recv()? {
                return Ok(Some(message));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            if self.wait_for_message_or_stream(Some(remaining))? == ReceiveWaitStatus::TimedOut {
                return Ok(None);
            }
        }
    }

    /// Subscriber runtime metadata
    pub fn stats(&self) -> Stats {
        let streams = match self.streams.try_borrow() {
            Ok(streams) => streams,
            Err(_) => return Stats::default(),
        };
        let mut stats = Stats::default();
        for subscriber_stream in streams.iter() {
            let stream_stats = subscriber_stream.stream.stats();
            stats.published_messages = stats
                .published_messages
                .saturating_add(stream_stats.published_messages);
            stats.received_messages = stats
                .received_messages
                .saturating_add(stream_stats.received_messages);
            stats.skipped_messages = stats
                .skipped_messages
                .saturating_add(stream_stats.skipped_messages);
            stats.warnings_total = stats
                .warnings_total
                .saturating_add(stream_stats.warnings_total);
            stats.connected_streams = stats.connected_streams.saturating_add(1);
            stats.disconnected_streams = stats
                .disconnected_streams
                .saturating_add(stream_stats.disconnected_streams);
            stats.publisher_alive |= stream_stats.publisher_alive;
        }
        stats
    }

    fn wait_for_message_or_stream(
        &self,
        timeout: Option<Duration>,
    ) -> Result<ReceiveWaitStatus, Warning> {
        let streams = self.attached_stream_snapshot()?;
        if streams.is_empty() {
            let notification = wait_stream_added(
                &self.socket_path,
                self.topic_hash,
                self.subscriber_id,
                self.last_seen_generation.get(),
                timeout,
            )?;
            if self.apply_stream_notification(notification)? {
                Ok(ReceiveWaitStatus::Woken)
            } else {
                Ok(ReceiveWaitStatus::TimedOut)
            }
        } else {
            wait_for_topic_wake(&self.topic_wait, self.subscriber_id, timeout)
        }
    }

    fn attached_stream_snapshot(&self) -> Result<Vec<Arc<StreamInner>>, Warning> {
        let streams = self.streams.try_borrow().map_err(|_| {
            Warning::new(
                WarningCode::InternalInvariantWarning,
                COMPONENT_RBUS,
                "Subscriber::attached_stream_snapshot::borrow",
            )
        })?;
        Ok(streams
            .iter()
            .map(|subscriber_stream| Arc::clone(&subscriber_stream.stream))
            .collect())
    }
}

struct SubscriberStream {
    stream_id: StreamId,
    stream: Arc<StreamInner>,
    cursor: Sequence,
}

struct TopicWaiterGuard<'a> {
    topic_wait: &'a TopicWaitSegment,
    waiter_mask: u64,
}

impl<'a> TopicWaiterGuard<'a> {
    fn new(topic_wait: &'a TopicWaitSegment, subscriber_id: SubscriberId) -> Result<Self, Warning> {
        let waiter_mask = reader_bit_mask(subscriber_id)?;
        topic_wait.add_waiter(waiter_mask);
        Ok(Self {
            topic_wait,
            waiter_mask,
        })
    }
}

impl Drop for TopicWaiterGuard<'_> {
    fn drop(&mut self) {
        self.topic_wait.remove_waiter(self.waiter_mask);
    }
}

fn wait_for_topic_wake(
    topic_wait: &TopicWaitSegment,
    subscriber_id: SubscriberId,
    timeout: Option<Duration>,
) -> Result<ReceiveWaitStatus, Warning> {
    let expected_epoch = topic_wait.wake_epoch();
    let _waiter_guard = TopicWaiterGuard::new(topic_wait, subscriber_id)?;
    let wait_status = futex_wait_shared(topic_wait.wake_epoch_word(), expected_epoch, timeout)?;
    Ok(match wait_status {
        FutexWaitStatus::TimedOut => ReceiveWaitStatus::TimedOut,
        FutexWaitStatus::Woken | FutexWaitStatus::ValueChanged => ReceiveWaitStatus::Woken,
    })
}
