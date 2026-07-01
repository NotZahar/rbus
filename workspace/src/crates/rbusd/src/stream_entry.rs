/// Stream id stored in daemon diagnostics
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StreamEntry {
    /// Stream id
    pub stream_id: u64,
}
