//! Cache-line alignment helpers
use crate::constants::CACHE_LINE_BYTES;

/// Wrapper that places the wrapped value on a dedicated cache line
#[repr(align(64))]
pub struct CachePadded<T> {
    value: T,
}

impl<T> CachePadded<T> {
    /// Wraps a value in cache-line padding
    pub const fn new(value: T) -> Self {
        Self { value }
    }

    /// Borrows the wrapped value
    pub const fn get(&self) -> &T {
        &self.value
    }

    /// Mutably borrows the wrapped value
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

/// Rounds `value` up to the next multiple of a power-of-two alignment
pub const fn align_value_to(value: usize, alignment: usize) -> Option<usize> {
    if alignment == 0 || !alignment.is_power_of_two() {
        return None;
    }
    let mask = alignment - 1;
    match value.checked_add(mask) {
        Some(added_value) => Some(added_value & !mask),
        None => None,
    }
}

/// Rounds `value` up to the next cache-line boundary
pub const fn align_value_to_cache_line(value: usize) -> Option<usize> {
    align_value_to(value, CACHE_LINE_BYTES)
}
