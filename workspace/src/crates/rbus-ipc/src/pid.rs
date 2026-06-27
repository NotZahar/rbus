//! PID helpers
/// Returns a value
pub fn current_process_id() -> u32 {
    std::process::id()
}
