/// Subscriber id stored in daemon diagnostics
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SubscriberEntry {
    /// Subscriber id
    pub subscriber_id: u8,
}
