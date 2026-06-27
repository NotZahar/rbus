//! Internal documentation
//!
//! Warning model metadata
//! Shared memory ABI metadata

pub mod atomic;
pub mod cache;
pub mod constants;
pub mod ids;
pub mod layout;
pub mod result;
pub mod ring;
pub mod sequence;
pub mod stats;
pub mod topic;
pub mod warning;

pub use result::RbusResult;
pub use sequence::Sequence;
pub use stats::Stats;
pub use warning::{Warning, WarningCode};
