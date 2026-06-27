//! Cache-line alignment helpers
use crate::constants::CACHE_LINE_BYTES;

/// Internal documentation
#[repr(align(64))]
pub struct CachePadded<T> {
    value: T,
}

impl<T> CachePadded<T> {
    /// Creates a value
    pub const fn new(value: T) -> Self {
        Self { value }
    }

    /// Returns a value
    pub const fn get(&self) -> &T {
        &self.value
    }

    /// Returns a value
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

/// Internal documentation
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

/// Internal documentation
pub const fn align_value_to_cache_line(value: usize) -> Option<usize> {
    align_value_to(value, CACHE_LINE_BYTES)
}
