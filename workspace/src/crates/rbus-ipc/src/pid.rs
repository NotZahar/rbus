//! PID helpers
/// Returns the current process id
pub fn current_process_id() -> u32 {
    std::process::id()
}
