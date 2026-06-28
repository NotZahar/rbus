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
use rbus_ipc::fd::OwnedFileDescriptor;
use rbus_ipc::futex::{FutexWaitStatus, futex_wait_shared};
use rbus_ipc::topic_wait::TopicWaitSegment;

use crate::control::{
    AttachedStream, attach_streams, notify_stream_added, reregister_subscriber, wait_stream_added,
};
use crate::message_ref::MessageRef;
use crate::stream::{ReadAttempt, StreamInner};
use crate::wait::ReceiveWaitStatus;

const COMPONENT_RBUS: &str = "rbus";

/// Subscriber runtime metadata
pub struct Subscriber {
    socket_path: PathBuf,
    topic_hash: TopicHash,
    subscriber_id: Cell<SubscriberId>,
    subscribed_generation: Cell<u64>,
    last_seen_generation: Cell<u64>,
    last_checked_stream_epoch: Cell<u32>,
    streams: RefCell<Vec<SubscriberStream>>,
    next_stream_index: Cell<usize>,
    topic_wait: RefCell<Arc<TopicWaitSegment>>,
    topic_wait_file_descriptor: RefCell<OwnedFileDescriptor>,
    control_client: RefCell<ControlClient>,
}

impl Subscriber {
    pub(crate) fn new(
        socket_path: PathBuf,
        topic_hash: TopicHash,
        subscriber_id: SubscriberId,
        subscribed_generation: u64,
        topic_wait: Arc<TopicWaitSegment>,
        topic_wait_file_descriptor: OwnedFileDescriptor,
        control_client: ControlClient,
    ) -> Self {
        let stream_epoch = topic_wait.stream_epoch();
        Self {
            socket_path,
            topic_hash,
            subscriber_id: Cell::new(subscriber_id),
            subscribed_generation: Cell::new(subscribed_generation),
            last_seen_generation: Cell::new(subscribed_generation),
            last_checked_stream_epoch: Cell::new(stream_epoch),
            streams: RefCell::new(Vec::new()),
            next_stream_index: Cell::new(0),
            topic_wait: RefCell::new(topic_wait),
            topic_wait_file_descriptor: RefCell::new(topic_wait_file_descriptor),
            control_client: RefCell::new(control_client),
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
            self.subscriber_id.get(),
            self.last_seen_generation.get(),
        );
        let notification = match notification {
            Ok(notification) => notification,
            Err(warning) if is_reconnect_warning(&warning) => {
                self.reregister_control_plane()?;
                return Ok(());
            }
            Err(warning) => return Err(warning),
        };
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
                let cursor = if stream.created_generation() > self.subscribed_generation.get() {
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
        let observed_stream_epoch = self.topic_wait_stream_epoch()?;
        if observed_stream_epoch != self.last_checked_stream_epoch.get() {
            match self.refresh_streams() {
                Ok(()) => {}
                Err(warning)
                    if is_reconnect_warning(&warning) && self.attached_stream_count()? > 0 => {}
                Err(warning) => return Err(warning),
            }
            self.last_checked_stream_epoch
                .set(self.topic_wait_stream_epoch()?);
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

        let stream_count = streams.len();
        let start_index = self.next_stream_index.get() % stream_count;
        for offset in 0..stream_count {
            let stream_index = (start_index + offset) % stream_count;
            let subscriber_stream = &mut streams[stream_index];
            match subscriber_stream
                .stream
                .try_read_message(subscriber_stream.cursor, self.subscriber_id.get())?
            {
                ReadAttempt::Empty => {}
                ReadAttempt::Message(read_reservation) => {
                    subscriber_stream.cursor = subscriber_stream.cursor.next();
                    self.next_stream_index
                        .set((stream_index + 1) % stream_count);
                    return Ok(Some(MessageRef::from_reservation(self, read_reservation)));
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
        match self.refresh_streams() {
            Ok(()) => {}
            Err(warning) if is_reconnect_warning(&warning) && self.attached_stream_count()? > 0 => {
            }
            Err(warning) => return Err(warning),
        }
        if self.attached_stream_count()? == 0 {
            let notification = wait_stream_added(
                &self.socket_path,
                self.topic_hash,
                self.subscriber_id.get(),
                self.last_seen_generation.get(),
                timeout,
            )?;
            if self.apply_stream_notification(notification)? {
                Ok(ReceiveWaitStatus::Woken)
            } else {
                Ok(ReceiveWaitStatus::TimedOut)
            }
        } else {
            self.wait_for_topic_wake(timeout)
        }
    }

    fn attached_stream_count(&self) -> Result<usize, Warning> {
        let streams = self.streams.try_borrow().map_err(|_| {
            Warning::new(
                WarningCode::InternalInvariantWarning,
                COMPONENT_RBUS,
                "Subscriber::attached_stream_count::borrow",
            )
        })?;
        Ok(streams.len())
    }

    fn wait_for_topic_wake(&self, timeout: Option<Duration>) -> Result<ReceiveWaitStatus, Warning> {
        let topic_wait = self.topic_wait_arc()?;
        let _waiter_guard = TopicWaiterGuard::new(&topic_wait, self.subscriber_id.get())?;
        let expected_epoch = topic_wait.wake_epoch();
        if self.has_waitable_progress()? {
            return Ok(ReceiveWaitStatus::Woken);
        }
        let wait_status = futex_wait_shared(topic_wait.wake_epoch_word(), expected_epoch, timeout)?;
        Ok(match wait_status {
            FutexWaitStatus::TimedOut => ReceiveWaitStatus::TimedOut,
            FutexWaitStatus::Woken | FutexWaitStatus::ValueChanged => ReceiveWaitStatus::Woken,
        })
    }

    fn has_waitable_progress(&self) -> Result<bool, Warning> {
        if self.topic_wait_stream_epoch()? != self.last_checked_stream_epoch.get() {
            return Ok(true);
        }
        let streams = self.streams.try_borrow().map_err(|_| {
            Warning::new(
                WarningCode::InternalInvariantWarning,
                COMPONENT_RBUS,
                "Subscriber::has_waitable_progress::borrow",
            )
        })?;
        Ok(streams.iter().any(|subscriber_stream| {
            subscriber_stream.stream.current_write_sequence() > subscriber_stream.cursor
        }))
    }

    fn reregister_control_plane(&self) -> Result<(), Warning> {
        let registration = {
            let topic_wait_file_descriptor =
                self.topic_wait_file_descriptor.try_borrow().map_err(|_| {
                    Warning::new(
                        WarningCode::InternalInvariantWarning,
                        COMPONENT_RBUS,
                        "Subscriber::reregister_control_plane::fd_borrow",
                    )
                })?;
            reregister_subscriber(
                &self.socket_path,
                self.topic_hash,
                &topic_wait_file_descriptor,
            )?
        };
        let topic_wait = Arc::new(TopicWaitSegment::map(
            &registration.topic_wait_file_descriptor,
        )?);
        self.subscriber_id.set(registration.subscriber_id);
        self.subscribed_generation
            .set(registration.subscribed_generation);
        self.last_seen_generation
            .set(registration.subscribed_generation);
        self.last_checked_stream_epoch
            .set(topic_wait.stream_epoch());
        *self.topic_wait.try_borrow_mut().map_err(|_| {
            Warning::new(
                WarningCode::InternalInvariantWarning,
                COMPONENT_RBUS,
                "Subscriber::reregister_control_plane::topic_wait_borrow",
            )
        })? = topic_wait;
        *self
            .topic_wait_file_descriptor
            .try_borrow_mut()
            .map_err(|_| {
                Warning::new(
                    WarningCode::InternalInvariantWarning,
                    COMPONENT_RBUS,
                    "Subscriber::reregister_control_plane::fd_borrow_mut",
                )
            })? = registration.topic_wait_file_descriptor;
        *self.control_client.try_borrow_mut().map_err(|_| {
            Warning::new(
                WarningCode::InternalInvariantWarning,
                COMPONENT_RBUS,
                "Subscriber::reregister_control_plane::client_borrow",
            )
        })? = registration.control_client;
        self.attach_existing_streams()
    }

    fn topic_wait_stream_epoch(&self) -> Result<u32, Warning> {
        Ok(self
            .topic_wait
            .try_borrow()
            .map_err(|_| {
                Warning::new(
                    WarningCode::InternalInvariantWarning,
                    COMPONENT_RBUS,
                    "Subscriber::topic_wait_stream_epoch",
                )
            })?
            .stream_epoch())
    }

    fn topic_wait_arc(&self) -> Result<Arc<TopicWaitSegment>, Warning> {
        let topic_wait = self.topic_wait.try_borrow().map_err(|_| {
            Warning::new(
                WarningCode::InternalInvariantWarning,
                COMPONENT_RBUS,
                "Subscriber::topic_wait_arc",
            )
        })?;
        Ok(Arc::clone(&topic_wait))
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

fn is_reconnect_warning(warning: &Warning) -> bool {
    matches!(
        warning.code(),
        WarningCode::DaemonUnavailable | WarningCode::Disconnected
    )
}
