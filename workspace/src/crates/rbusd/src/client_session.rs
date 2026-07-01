/// Control-plane session tracked by the daemon
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClientSession {
    /// Session id
    pub session_id: u64,
}
