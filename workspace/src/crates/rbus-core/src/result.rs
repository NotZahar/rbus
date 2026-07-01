//! Shared result type for recoverable rbus warnings

use crate::warning::Warning;

/// Result alias used by APIs that return structured warnings
pub type RbusResult<T> = core::result::Result<T, Warning>;
