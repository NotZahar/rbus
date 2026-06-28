//! Subscriber API
use core::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rbus_core::ids::{SubscriberId, TopicHash};
use rbus_core::ring::pin::reader_bit_mask;
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
use crate::subscriber_streams::{SubscriberStreamRead, SubscriberStreams};
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
    streams: SubscriberStreams,
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
            streams: SubscriberStreams::new(),
            topic_wait: RefCell::new(topic_wait),
            topic_wait_file_descriptor: RefCell::new(topic_wait_file_descriptor),
            control_client: RefCell::new(control_client),
        }
    }

    pub(crate) fn attach_existing_streams(&self) -> Result<(), Warning> {
        let attached_streams = attach_streams(&self.socket_path, self.topic_hash)?;
        let update = self.attach_streams_to_set(attached_streams)?;
        self.last_seen_generation
            .set(update.max_generation.max(self.last_seen_generation.get()));
        Ok(())
    }

    pub(crate) fn refresh_streams(&self) -> Result<bool, Warning> {
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
                return Ok(false);
            }
            Err(warning) => return Err(warning),
        };
        self.apply_stream_notification(notification)
    }

    fn apply_stream_notification(
        &self,
        notification: crate::control::StreamNotification,
    ) -> Result<bool, Warning> {
        let update = self.attach_streams_to_set(notification.streams)?;
        self.last_seen_generation.set(
            notification
                .latest_generation
                .max(update.max_generation)
                .max(self.last_seen_generation.get()),
        );
        Ok(update.has_new_stream)
    }

    fn attach_streams_to_set(
        &self,
        attached_streams: Vec<AttachedStream>,
    ) -> Result<crate::subscriber_streams::AttachedStreamSetUpdate, Warning> {
        self.streams.attach(
            attached_streams,
            self.subscribed_generation.get(),
            self.last_seen_generation.get(),
        )
    }

    /// Internal documentation
    pub fn try_recv(&self) -> Result<Option<MessageRef<'_>>, Warning> {
        match self
            .streams
            .try_recv_single_stream(self.subscriber_id.get(), |read_reservation| {
                MessageRef::from_reservation(self, read_reservation)
            })? {
            SubscriberStreamRead::Message(message) => {
                return Ok(Some(message));
            }
            SubscriberStreamRead::Skipped(warning) => return Err(warning),
            SubscriberStreamRead::Empty => {
                if self.refresh_streams_if_epoch_changed()? {
                    return self.try_recv_from_attached_streams();
                }
                return Ok(None);
            }
            SubscriberStreamRead::NotSingleStream => {}
        }
        self.refresh_streams_if_epoch_changed()?;
        self.try_recv_from_attached_streams()
    }

    fn try_recv_from_attached_streams(&self) -> Result<Option<MessageRef<'_>>, Warning> {
        match self
            .streams
            .try_recv_round_robin(self.subscriber_id.get(), |read_reservation| {
                MessageRef::from_reservation(self, read_reservation)
            })? {
            SubscriberStreamRead::Message(message) => Ok(Some(message)),
            SubscriberStreamRead::Skipped(warning) => Err(warning),
            SubscriberStreamRead::Empty | SubscriberStreamRead::NotSingleStream => Ok(None),
        }
    }

    fn refresh_streams_if_epoch_changed(&self) -> Result<bool, Warning> {
        let observed_stream_epoch = self.topic_wait_stream_epoch()?;
        if observed_stream_epoch == self.last_checked_stream_epoch.get() {
            return Ok(false);
        }
        let has_new_stream = match self.refresh_streams() {
            Ok(has_new_stream) => has_new_stream,
            Err(warning) if is_reconnect_warning(&warning) && self.attached_stream_count() > 0 => {
                false
            }
            Err(warning) => return Err(warning),
        };
        self.last_checked_stream_epoch
            .set(self.topic_wait_stream_epoch()?);
        Ok(has_new_stream)
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
        match self.streams.stats() {
            Ok(stats) => stats,
            Err(warning) => panic!("Subscriber::stats invariant warning: {warning:?}"),
        }
    }

    fn wait_for_message_or_stream(
        &self,
        timeout: Option<Duration>,
    ) -> Result<ReceiveWaitStatus, Warning> {
        match self.refresh_streams() {
            Ok(_) => {}
            Err(warning) if is_reconnect_warning(&warning) && self.attached_stream_count() > 0 => {}
            Err(warning) => return Err(warning),
        }
        if self.attached_stream_count() == 0 {
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

    fn attached_stream_count(&self) -> usize {
        self.streams.count()
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
        self.streams.has_waitable_progress()
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
