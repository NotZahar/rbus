//! Subscriber attached stream set
use core::cell::{Cell, RefCell};
use core::ptr::NonNull;
use std::sync::Arc;

use rbus_core::ids::SubscriberId;
use rbus_core::sequence::Sequence;
use rbus_core::warning::{Warning, WarningCode};

use crate::control::AttachedStream;
use crate::stream::{ReadAttempt, ReadReservation, StreamInner};

const COMPONENT_RBUS: &str = "rbus";
const INITIAL_STREAM_SEQUENCE: Sequence = Sequence::new(0);
const FIRST_STREAM_INDEX: usize = 0;
const SINGLE_STREAM_COUNT: usize = 1;
const NEXT_STREAM_INDEX_ADVANCE: usize = 1;

pub(crate) struct SubscriberStreams {
    streams: RefCell<Vec<SubscriberStream>>,
    next_stream_index: Cell<usize>,
    stream_count: Cell<usize>,
    single_stream_pointer: Cell<Option<NonNull<StreamInner>>>,
    single_stream_cursor: Cell<Sequence>,
}

impl SubscriberStreams {
    pub(crate) fn new() -> Self {
        Self {
            streams: RefCell::new(Vec::new()),
            next_stream_index: Cell::new(FIRST_STREAM_INDEX),
            stream_count: Cell::new(0),
            single_stream_pointer: Cell::new(None),
            single_stream_cursor: Cell::new(INITIAL_STREAM_SEQUENCE),
        }
    }

    pub(crate) fn attach(
        &self,
        attached_streams: Vec<AttachedStream>,
        subscribed_generation: u64,
        last_seen_generation: u64,
    ) -> Result<AttachedStreamSetUpdate, Warning> {
        let mut streams = self.streams.try_borrow_mut().map_err(|_| {
            Warning::new(
                WarningCode::InternalInvariantWarning,
                COMPONENT_RBUS,
                "SubscriberStreams::attach::borrow",
            )
        })?;
        self.sync_single_cursor(&mut streams);

        let mut max_generation = last_seen_generation;
        let mut has_new_stream = false;
        for attached_stream in attached_streams {
            if streams.iter().any(|existing_stream| {
                existing_stream.stream_id == attached_stream.metadata.stream_id
            }) {
                continue;
            }

            max_generation = max_generation.max(attached_stream.stream_generation);
            let stream = StreamInner::attach(
                attached_stream.metadata,
                attached_stream.stream_generation,
                &attached_stream.file_descriptor,
                0,
            )?;
            let cursor = if stream.created_generation() > subscribed_generation {
                INITIAL_STREAM_SEQUENCE
            } else {
                stream.current_write_sequence()
            };
            streams.push(SubscriberStream {
                stream_id: stream.stream_id(),
                cursor,
                stream,
            });
            has_new_stream = true;
        }

        self.rebuild_single_stream_fast_path(&streams);
        Ok(AttachedStreamSetUpdate {
            max_generation,
            has_new_stream,
        })
    }

    pub(crate) fn try_recv_single_stream<Message>(
        &self,
        subscriber_id: SubscriberId,
        mut create_message: impl for<'stream> FnMut(ReadReservation<'stream>) -> Message,
    ) -> Result<SubscriberStreamRead<Message>, Warning> {
        let Some(stream_pointer) = self.single_stream_pointer.get() else {
            return Ok(SubscriberStreamRead::NotSingleStream);
        };
        // SAFETY: SubscriberStreams keeps the StreamInner Arc in `streams` while the
        // single-stream pointer is enabled. Attached streams are never removed from this set
        let stream = unsafe { stream_pointer.as_ref() };
        match stream.try_read_message(self.single_stream_cursor.get(), subscriber_id)? {
            ReadAttempt::Empty => Ok(SubscriberStreamRead::Empty),
            ReadAttempt::Message(read_reservation) => {
                self.single_stream_cursor
                    .set(read_reservation.sequence.next());
                Ok(SubscriberStreamRead::Message(create_message(
                    read_reservation,
                )))
            }
            ReadAttempt::Skipped {
                next_sequence,
                warning,
            } => {
                self.single_stream_cursor.set(next_sequence);
                Ok(SubscriberStreamRead::Skipped(warning))
            }
        }
    }

    pub(crate) fn try_recv_round_robin<Message>(
        &self,
        subscriber_id: SubscriberId,
        mut create_message: impl for<'stream> FnMut(ReadReservation<'stream>) -> Message,
    ) -> Result<SubscriberStreamRead<Message>, Warning> {
        let mut streams = self.streams.try_borrow_mut().map_err(|_| {
            Warning::new(
                WarningCode::InternalInvariantWarning,
                COMPONENT_RBUS,
                "SubscriberStreams::try_recv_round_robin::borrow",
            )
        })?;
        if streams.is_empty() {
            return Ok(SubscriberStreamRead::Empty);
        }

        let stream_count = streams.len();
        let start_index = self.next_stream_index.get() % stream_count;
        for offset in 0..stream_count {
            let stream_index = (start_index + offset) % stream_count;
            let subscriber_stream = &mut streams[stream_index];
            match subscriber_stream
                .stream
                .try_read_message(subscriber_stream.cursor, subscriber_id)?
            {
                ReadAttempt::Empty => {}
                ReadAttempt::Message(read_reservation) => {
                    let next_cursor = subscriber_stream.cursor.next();
                    subscriber_stream.cursor = next_cursor;
                    if stream_count == SINGLE_STREAM_COUNT {
                        self.single_stream_cursor.set(next_cursor);
                    }
                    self.next_stream_index
                        .set((stream_index + NEXT_STREAM_INDEX_ADVANCE) % stream_count);
                    return Ok(SubscriberStreamRead::Message(create_message(
                        read_reservation,
                    )));
                }
                ReadAttempt::Skipped {
                    next_sequence,
                    warning,
                } => {
                    subscriber_stream.cursor = next_sequence;
                    if stream_count == SINGLE_STREAM_COUNT {
                        self.single_stream_cursor.set(next_sequence);
                    }
                    self.next_stream_index.set(stream_index);
                    return Ok(SubscriberStreamRead::Skipped(warning));
                }
            }
        }
        Ok(SubscriberStreamRead::Empty)
    }

    pub(crate) fn has_waitable_progress(&self) -> Result<bool, Warning> {
        if let Some(stream_pointer) = self.single_stream_pointer.get() {
            // SAFETY: The enabled single-stream pointer is backed by an Arc kept in `streams`
            let stream = unsafe { stream_pointer.as_ref() };
            return Ok(stream.current_write_sequence() > self.single_stream_cursor.get());
        }

        let streams = self.streams.try_borrow().map_err(|_| {
            Warning::new(
                WarningCode::InternalInvariantWarning,
                COMPONENT_RBUS,
                "SubscriberStreams::has_waitable_progress::borrow",
            )
        })?;
        Ok(streams.iter().any(|subscriber_stream| {
            subscriber_stream.stream.current_write_sequence() > subscriber_stream.cursor
        }))
    }

    pub(crate) fn stats(&self) -> Result<rbus_core::stats::Stats, Warning> {
        let streams = self.streams.try_borrow().map_err(|_| {
            Warning::new(
                WarningCode::InternalInvariantWarning,
                COMPONENT_RBUS,
                "SubscriberStreams::stats::borrow",
            )
        })?;
        let mut stats = rbus_core::stats::Stats::default();
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
            stats.connected_streams = stats
                .connected_streams
                .saturating_add(SINGLE_STREAM_COUNT as u64);
            stats.disconnected_streams = stats
                .disconnected_streams
                .saturating_add(stream_stats.disconnected_streams);
            stats.publisher_alive |= stream_stats.publisher_alive;
        }
        Ok(stats)
    }

    pub(crate) fn count(&self) -> usize {
        self.stream_count.get()
    }

    fn sync_single_cursor(&self, streams: &mut [SubscriberStream]) {
        if self.single_stream_pointer.get().is_some() && streams.len() == SINGLE_STREAM_COUNT {
            streams[FIRST_STREAM_INDEX].cursor = self.single_stream_cursor.get();
        }
    }

    fn rebuild_single_stream_fast_path(&self, streams: &[SubscriberStream]) {
        self.stream_count.set(streams.len());
        if streams.len() == SINGLE_STREAM_COUNT {
            self.single_stream_cursor
                .set(streams[FIRST_STREAM_INDEX].cursor);
            self.single_stream_pointer
                .set(Some(NonNull::from(Arc::as_ref(
                    &streams[FIRST_STREAM_INDEX].stream,
                ))));
        } else {
            self.single_stream_pointer.set(None);
        }
    }
}

pub(crate) struct AttachedStreamSetUpdate {
    pub(crate) max_generation: u64,
    pub(crate) has_new_stream: bool,
}

pub(crate) enum SubscriberStreamRead<Message> {
    NotSingleStream,
    Empty,
    Message(Message),
    Skipped(Warning),
}

struct SubscriberStream {
    stream_id: rbus_core::ids::StreamId,
    stream: Arc<StreamInner>,
    cursor: Sequence,
}
