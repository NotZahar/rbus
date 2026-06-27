//! Internal documentation

use core::marker::PhantomData;

use rbus_core::Sequence;

use crate::publisher::Publisher;
use crate::stream::LoanReservation;
use crate::stream::StreamInner;

/// Stream runtime metadata
pub struct LoanedMessage<'publisher> {
    pub(crate) stream: std::sync::Arc<StreamInner>,
    pub(crate) publisher_id: rbus_core::ids::PublisherId,
    pub(crate) slot_index: usize,
    pub(crate) sequence: Sequence,
    pub(crate) length: usize,
    pub(crate) payload_pointer: *mut u8,
    pub(crate) published: bool,
    pub(crate) publisher_lifetime: PhantomData<&'publisher Publisher>,
}

impl<'publisher> LoanedMessage<'publisher> {
    pub(crate) fn from_reservation(
        publisher: &'publisher Publisher,
        reservation: LoanReservation,
    ) -> Self {
        Self {
            publisher_id: publisher.publisher_id(),
            stream: reservation.stream,
            slot_index: reservation.slot_index,
            sequence: reservation.sequence,
            length: reservation.length,
            payload_pointer: reservation.payload_pointer,
            published: false,
            publisher_lifetime: PhantomData,
        }
    }

    /// Payload metadata
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: Required invariants are upheld
        // Invariants:
        // - Required invariant holds
        // - Required invariant holds
        // Covered by tests:
        // - loaned_publish_makes_payload_visible_to_subscriber
        unsafe { core::slice::from_raw_parts_mut(self.payload_pointer, self.length) }
    }

    pub(crate) fn mark_published(&mut self) {
        self.published = true;
    }
}

impl Drop for LoanedMessage<'_> {
    fn drop(&mut self) {
        if !self.published {
            self.stream.rollback_loan(self.slot_index);
        }
    }
}
