//! Read-only message reference
use core::marker::PhantomData;
use core::ptr::NonNull;

use rbus_core::Sequence;

use crate::stream::{ReadReservation, StreamInner};
use crate::subscriber::Subscriber;

/// Borrowed message returned by a subscriber receive operation
pub struct MessageRef<'subscriber> {
    stream: NonNull<StreamInner>,
    slot_index: usize,
    reader_bit: u64,
    sequence: Sequence,
    length: usize,
    timestamp_ns: u64,
    payload_pointer: *const u8,
    subscriber_lifetime: PhantomData<&'subscriber Subscriber>,
}

impl<'subscriber> MessageRef<'subscriber> {
    pub(crate) fn from_reservation(
        _subscriber: &'subscriber Subscriber,
        read_reservation: ReadReservation<'_>,
    ) -> Self {
        Self {
            stream: read_reservation.stream,
            slot_index: read_reservation.slot_index,
            reader_bit: read_reservation.reader_bit,
            sequence: read_reservation.sequence,
            length: read_reservation.length,
            timestamp_ns: read_reservation.timestamp_ns,
            payload_pointer: read_reservation.payload_pointer,
            subscriber_lifetime: PhantomData,
        }
    }

    /// Returns the received payload bytes
    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: ReadReservation pins the slot until MessageRef::drop, and Subscriber keeps
        // the mapped stream alive for the whole 'subscriber lifetime
        unsafe { core::slice::from_raw_parts(self.payload_pointer, self.length) }
    }

    /// Returns the stream sequence assigned by the publisher
    pub const fn sequence(&self) -> Sequence {
        self.sequence
    }

    /// Returns the publisher timestamp stored with the message
    pub const fn timestamp_ns(&self) -> u64 {
        self.timestamp_ns
    }
}

impl Drop for MessageRef<'_> {
    fn drop(&mut self) {
        // SAFETY: Subscriber keeps attached stream Arcs alive while MessageRef<'subscriber> exists
        unsafe {
            self.stream
                .as_ref()
                .unpin_reader(self.slot_index, self.reader_bit);
        }
    }
}
