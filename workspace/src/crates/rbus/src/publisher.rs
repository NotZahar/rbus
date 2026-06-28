//! Publisher API
use core::cell::Cell;
use core::marker::PhantomData;
use std::path::PathBuf;
use std::sync::Arc;

use rbus_core::sequence::Sequence;
use rbus_core::warning::Warning;

use rbus_ipc::control::client::ControlClient;
use rbus_ipc::fd::OwnedFileDescriptor;
use rbus_ipc::topic_wait::TopicWaitSegment;

use crate::loaned::LoanedMessage;
use crate::reregister::PublisherReregisterWorker;
use crate::stats::Stats;
use crate::stream::StreamInner;

/// Publisher runtime metadata
pub struct Publisher {
    _reregister_worker: PublisherReregisterWorker,
    stream: Arc<StreamInner>,
    topic_wait: Arc<TopicWaitSegment>,
    not_sync: PhantomData<Cell<()>>,
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
            not_sync: PhantomData,
        })
    }

    /// Stream runtime metadata
    pub fn loan(&mut self, length: usize) -> Result<LoanedMessage<'_>, Warning> {
        let reservation = self.stream.try_begin_loan(length)?;
        Ok(LoanedMessage::from_reservation(self, reservation))
    }

    /// Internal documentation
    pub fn publish_bytes(&mut self, bytes: &[u8]) -> Result<Sequence, Warning> {
        let mut message = self.loan(bytes.len())?;
        message.as_mut_slice().copy_from_slice(bytes);
        message.publish()
    }

    /// Publisher runtime metadata
    pub fn stats(&self) -> Stats {
        self.stream.stats()
    }

    pub(crate) fn commit_loan(
        &self,
        slot_index: usize,
        sequence: Sequence,
        length: usize,
    ) -> Result<(), Warning> {
        self.stream.commit_loan(slot_index, sequence, length)
    }

    pub(crate) fn rollback_loan(&self, slot_index: usize) {
        self.stream.rollback_loan(slot_index);
    }

    pub(crate) fn wake_waiters(&self) -> Result<(), Warning> {
        self.topic_wait.wake_waiters()
    }
}
