//! Publisher API
use std::sync::Arc;

use rbus_core::ids::PublisherId;
use rbus_core::warning::{Warning, WarningCode};
use rbus_ipc::control::client::ControlClient;
use rbus_ipc::topic_wait::TopicWaitSegment;

use crate::loaned::LoanedMessage;
use crate::sequence::Sequence;
use crate::stats::Stats;
use crate::stream::StreamInner;

const COMPONENT_RBUS: &str = "rbus";

/// Publisher runtime metadata
pub struct Publisher {
    stream: Arc<StreamInner>,
    topic_wait: Arc<TopicWaitSegment>,
    _control_client: ControlClient,
}

impl Publisher {
    pub(crate) fn new(
        stream: Arc<StreamInner>,
        topic_wait: Arc<TopicWaitSegment>,
        control_client: ControlClient,
    ) -> Self {
        Self {
            stream,
            topic_wait,
            _control_client: control_client,
        }
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
