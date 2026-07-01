/// Publisher id stored in daemon diagnostics
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PublisherEntry {
    /// Publisher id
    pub publisher_id: u64,
}
