//! Zero-copy publisher loaned-message API

use rbus_core::Sequence;
use rbus_core::warning::Warning;

use crate::publisher::Publisher;
use crate::stream::LoanReservation;

/// Writable message loan that must be published or rolled back on drop
pub struct LoanedMessage<'publisher> {
    pub(crate) publisher: &'publisher mut Publisher,
    pub(crate) slot_index: usize,
    pub(crate) sequence: Sequence,
    pub(crate) length: usize,
    pub(crate) payload_pointer: *mut u8,
    pub(crate) published: bool,
}

impl<'publisher> LoanedMessage<'publisher> {
    pub(crate) fn from_reservation(
        publisher: &'publisher mut Publisher,
        reservation: LoanReservation,
    ) -> Self {
        Self {
            publisher,
            slot_index: reservation.slot_index,
            sequence: reservation.sequence,
            length: reservation.length,
            payload_pointer: reservation.payload_pointer,
            published: false,
        }
    }

    /// Returns the writable payload buffer for this loan
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: LoanReservation points into a claimed slot, length was validated against the
        // slot size, and the &mut Publisher borrow prevents another loan from aliasing it
        unsafe { core::slice::from_raw_parts_mut(self.payload_pointer, self.length) }
    }

    pub(crate) fn mark_published(&mut self) {
        self.published = true;
    }

    /// Commits this loaned message to the publisher stream it came from
    ///
    /// ```compile_fail
    /// use rbus::{Bus, TopicConfig};
    ///
    /// fn cannot_start_second_loan_before_first_is_finished(bus: &Bus) -> rbus::RbusResult<()> {
    ///     let mut publisher = bus.publisher("/typed/loan", TopicConfig::new(8, 4))?;
    ///     let first_message = publisher.loan(1)?;
    ///     let second_message = publisher.loan(1)?;
    ///     drop(first_message);
    ///     drop(second_message);
    ///     Ok(())
    /// }
    /// ```
    ///
    /// ```compile_fail
    /// use rbus::{Bus, TopicConfig};
    ///
    /// fn cannot_publish_loan_through_another_publisher(bus: &Bus) -> rbus::RbusResult<()> {
    ///     let mut first_publisher = bus.publisher("/typed/source", TopicConfig::new(8, 4))?;
    ///     let mut second_publisher = bus.publisher("/typed/target", TopicConfig::new(8, 4))?;
    ///     let message = first_publisher.loan(1)?;
    ///     second_publisher.publish(message)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn publish(mut self) -> Result<Sequence, Warning> {
        let sequence = self.sequence;
        let commit_result = self
            .publisher
            .commit_loan(self.slot_index, self.sequence, self.length);
        self.mark_published();
        commit_result?;
        self.publisher.wake_waiters()?;
        Ok(sequence)
    }
}

impl Drop for LoanedMessage<'_> {
    fn drop(&mut self) {
        if !self.published {
            self.publisher.rollback_loan(self.slot_index);
        }
    }
}
