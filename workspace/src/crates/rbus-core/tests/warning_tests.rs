use rbus_core::ids::{StreamId, TopicHash};
use rbus_core::sequence::Sequence;
use rbus_core::warning::{Warning, WarningCode};

#[test]
fn every_warning_code_has_text() {
    for warning_code in WarningCode::all_codes() {
        assert!(
            !warning_code.description().is_empty(),
            "warning code should have text"
        );
    }
}

#[test]
fn warning_keeps_code_and_context_without_allocation_text() {
    let warning = Warning::new(
        WarningCode::MessageTooLarge,
        "rbus-core",
        "warning_keeps_code",
    );
    assert_eq!(warning.code(), WarningCode::MessageTooLarge);
    assert_eq!(warning.component(), "rbus-core");
    assert_eq!(warning.message(), "message exceeds slot size");
}

#[test]
fn warning_context_accessors_restore_compact_fields() {
    let warning = Warning::new(WarningCode::MessagesSkipped, "rbus-core", "warning_context")
        .with_os_errno(11)
        .with_sequence(Sequence::new(42))
        .with_topic_hash(TopicHash::new(43))
        .with_stream_id(StreamId::new(44))
        .with_monotonic_timestamp_ns(45);

    assert_eq!(warning.os_errno(), Some(11));
    assert_eq!(warning.sequence(), Some(Sequence::new(42)));
    assert_eq!(warning.topic_hash(), Some(TopicHash::new(43)));
    assert_eq!(warning.stream_id(), Some(StreamId::new(44)));
    assert_eq!(warning.monotonic_timestamp_ns(), Some(45));
}

#[test]
fn warning_fits_result_err_without_boxing() {
    assert!(
        core::mem::size_of::<Warning>() <= 96,
        "Warning should stay compact for RbusResult<T>"
    );
}
