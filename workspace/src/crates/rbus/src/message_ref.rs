//! Read-only message reference
use core::marker::PhantomData;

use rbus_core::Sequence;

use crate::stream::{ReadReservation, StreamInner};
use crate::subscriber::Subscriber;

/// Stream runtime metadata
pub struct MessageRef<'subscriber> {
    stream: std::sync::Arc<StreamInner>,
    slot_index: usize,
    reader_bit: u64,
    sequence: Sequence,
    length: usize,
    timestamp_ns: u64,
    payload_pointer: *const u8,
    subscriber_lifetime: PhantomData<&'subscriber Subscriber>,
}

impl<'subscriber> MessageRef<'subscriber> {
    pub(crate) fn from_reservation(read_reservation: ReadReservation) -> Self {
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

    /// Payload metadata
    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: Required invariants are upheld
        // Invariants:
        // - Required invariant holds
        // - Required invariant holds
        // Covered by tests:
        // - loaned_publish_makes_payload_visible_to_subscriber
        unsafe { core::slice::from_raw_parts(self.payload_pointer, self.length) }
    }

    /// Sequence metadata
    pub const fn sequence(&self) -> Sequence {
        self.sequence
    }

    /// Returns a value
    pub const fn timestamp_ns(&self) -> u64 {
        self.timestamp_ns
    }
}

impl Drop for MessageRef<'_> {
    fn drop(&mut self) {
        self.stream.unpin_reader(self.slot_index, self.reader_bit);
    }
}
