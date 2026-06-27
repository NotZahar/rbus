use core::sync::atomic::AtomicU32;
use std::time::Duration;

use rbus_core::warning::WarningCode;
use rbus_ipc::futex::{
    FutexWaitStatus, futex_wait_private, futex_wait_shared, futex_wake_private, futex_wake_shared,
};

#[test]
fn futex_wake_private_returns_zero_without_waiters() {
    let futex_word = AtomicU32::new(0);
    let woken_count = futex_wake_private(&futex_word, 1).expect("futex operation should succeed");

    assert_eq!(woken_count, 0);
}

#[test]
fn futex_wait_private_returns_warning_when_value_mismatches() {
    let futex_word = AtomicU32::new(1);
    let warning = match futex_wait_private(&futex_word, 0) {
        Ok(()) => panic!("operation should return warning"),
        Err(warning) => warning,
    };

    assert_eq!(warning.code(), WarningCode::SyscallFailed);
}

#[test]
fn futex_wake_shared_returns_zero_without_waiters() {
    let futex_word = AtomicU32::new(0);
    let woken_count = futex_wake_shared(&futex_word, 1).expect("futex operation should succeed");

    assert_eq!(woken_count, 0);
}

#[test]
fn futex_wait_shared_reports_value_change() {
    let futex_word = AtomicU32::new(1);
    let status = futex_wait_shared(&futex_word, 0, Some(Duration::from_millis(1)))
        .expect("futex operation should succeed");

    assert_eq!(status, FutexWaitStatus::ValueChanged);
}
