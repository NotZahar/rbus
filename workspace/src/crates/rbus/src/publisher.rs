//! Publisher API
use std::sync::Arc;

use rbus_core::ids::PublisherId;
use rbus_core::warning::{Warning, WarningCode};
use std::path::PathBuf;

use rbus_ipc::control::client::ControlClient;
use rbus_ipc::fd::OwnedFileDescriptor;
use rbus_ipc::topic_wait::TopicWaitSegment;

use crate::loaned::LoanedMessage;
use crate::reregister::PublisherReregisterWorker;
use crate::sequence::Sequence;
use crate::stats::Stats;
use crate::stream::StreamInner;

const COMPONENT_RBUS: &str = "rbus";

/// Publisher runtime metadata
pub struct Publisher {
    _reregister_worker: PublisherReregisterWorker,
    stream: Arc<StreamInner>,
    topic_wait: Arc<TopicWaitSegment>,
}

impl Publisher {
    pub(crate) fn new(
        socket_path: PathBuf,
        stream: Arc<StreamInner>,
        topic_wait: Arc<TopicWaitSegment>,
        stream_file_descriptor: OwnedFileDescriptor,
        topic_wait_file_descriptor: OwnedFileDescriptor,
        control_client: ControlClient,
    ) -> Result<Self, Warning> {
        let reregister_worker = PublisherReregisterWorker::start(
            socket_path,
            Arc::clone(&stream),
            stream_file_descriptor,
            topic_wait_file_descriptor,
            control_client,
        )?;
        Ok(Self {
            _reregister_worker: reregister_worker,
            stream,
            topic_wait,
        })
    }

    pub(crate) fn publisher_id(&self) -> PublisherId {
        self.stream.publisher_id()
    }

    /// Stream runtime metadata
    pub fn loan(&self, length: usize) -> Result<LoanedMessage<'_>, Warning> {
        let reservation = self.stream.try_begin_loan(length)?;
        Ok(LoanedMessage::from_reservation(self, reservation))
    }

    /// Internal documentation
    pub fn publish(&self, mut message: LoanedMessage<'_>) -> Result<Sequence, Warning> {
        if message.publisher_id != self.publisher_id() {
            message.stream.rollback_loan(message.slot_index);
            message.mark_published();
            return Err(Warning::new(
                WarningCode::InternalInvariantWarning,
                COMPONENT_RBUS,
                "Publisher::publish::publisher_id",
            ));
        }

        let sequence = message.sequence;
        let commit_result =
            self.stream
                .commit_loan(message.slot_index, message.sequence, message.length);
        message.mark_published();
        commit_result?;
        self.topic_wait.wake_waiters()?;
        Ok(sequence)
    }

    /// Internal documentation
    pub fn publish_bytes(&self, bytes: &[u8]) -> Result<Sequence, Warning> {
        let mut message = self.loan(bytes.len())?;
        message.as_mut_slice().copy_from_slice(bytes);
        self.publish(message)
    }

    /// Publisher runtime metadata
    pub fn stats(&self) -> Stats {
        self.stream.stats()
    }
}
