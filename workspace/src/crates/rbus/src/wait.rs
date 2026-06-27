//! Internal wait strategy
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReceiveWaitStatus {
    Woken,
    TimedOut,
}
