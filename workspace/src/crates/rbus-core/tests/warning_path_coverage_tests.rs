use rbus_core::warning::WarningCode;

struct WarningPathCoverage {
    code: WarningCode,
    test_name: &'static str,
    state_assertion: &'static str,
}

const WARNING_PATH_COVERAGE: &[WarningPathCoverage] = &[
    WarningPathCoverage {
        code: WarningCode::DaemonUnavailable,
        test_name: "bus_connect_requires_running_daemon",
        state_assertion: "missing socket path stays absent",
    },
    WarningPathCoverage {
        code: WarningCode::ProtocolMismatch,
        test_name: "protocol_mismatch_warning_path_rejects_unknown_opcode",
        state_assertion: "input frame bytes stay unchanged",
    },
    WarningPathCoverage {
        code: WarningCode::PermissionDenied,
        test_name: "permission_denied_warning_path_preserves_blocking_file",
        state_assertion: "blocking file remains a file",
    },
    WarningPathCoverage {
        code: WarningCode::SyscallFailed,
        test_name: "syscall_failed_warning_path_leaves_futex_state_unchanged",
        state_assertion: "futex word remains unchanged",
    },
    WarningPathCoverage {
        code: WarningCode::MmapFailed,
        test_name: "mmap_failed_warning_path_keeps_pipe_usable",
        state_assertion: "pipe descriptors remain usable until closed",
    },
    WarningPathCoverage {
        code: WarningCode::InvalidConfig,
        test_name: "invalid_config_warning_path_preserves_empty_runtime_state",
        state_assertion: "invalid domain creates no bus state",
    },
    WarningPathCoverage {
        code: WarningCode::MessageTooLarge,
        test_name: "message_too_large_warning_path_increments_total_only",
        state_assertion: "warnings_total increments without publishing",
    },
    WarningPathCoverage {
        code: WarningCode::BackpressurePinnedSlot,
        test_name: "backpressure_warning_path_increments_backpressure_counter",
        state_assertion: "pinned slot blocks overwrite until drop",
    },
    WarningPathCoverage {
        code: WarningCode::MessagesSkipped,
        test_name: "messages_skipped_warning_path_updates_cursor_and_counters",
        state_assertion: "cursor advances to oldest retained sequence",
    },
    WarningPathCoverage {
        code: WarningCode::Timeout,
        test_name: "bench_worker_recv_timeout_warning_path_is_fatal",
        state_assertion: "worker exits with warning status and stacktrace",
    },
    WarningPathCoverage {
        code: WarningCode::Disconnected,
        test_name: "disconnected_warning_path_increments_total_on_closed_stream",
        state_assertion: "stream stays closed and disconnected counter stays visible",
    },
    WarningPathCoverage {
        code: WarningCode::SegmentAbiMismatch,
        test_name: "segment_abi_mismatch_warning_path_preserves_original_segment",
        state_assertion: "valid original segment remains mappable",
    },
    WarningPathCoverage {
        code: WarningCode::SegmentQuarantined,
        test_name: "segment_quarantined_warning_path_preserves_context_state",
        state_assertion: "stream id context is preserved",
    },
    WarningPathCoverage {
        code: WarningCode::SubscriberLimitReached,
        test_name: "reader_bit_mask_reserves_writer_claim_bit",
        state_assertion: "reserved writer bit is not handed to a subscriber",
    },
    WarningPathCoverage {
        code: WarningCode::ResourceLimit,
        test_name: "resource_limit_warning_path_preserves_futex_state",
        state_assertion: "oversized wake count leaves futex word unchanged",
    },
    WarningPathCoverage {
        code: WarningCode::CleanupIncomplete,
        test_name: "cleanup_incomplete_warning_path_preserves_context_state",
        state_assertion: "cleanup operation context is preserved",
    },
    WarningPathCoverage {
        code: WarningCode::InternalInvariantWarning,
        test_name: "internal_invariant_warning_path_rolls_back_foreign_loan",
        state_assertion: "foreign loan rollback releases the original stream",
    },
];

#[test]
fn every_warning_code_has_reproducible_warning_path_coverage() {
    assert_eq!(WARNING_PATH_COVERAGE.len(), WarningCode::all_codes().len());
    for warning_code in WarningCode::all_codes() {
        let coverage = WARNING_PATH_COVERAGE
            .iter()
            .find(|coverage| coverage.code == *warning_code)
            .unwrap_or_else(|| panic!("missing warning path coverage for {warning_code:?}"));
        assert!(!coverage.test_name.is_empty());
        assert!(!coverage.state_assertion.is_empty());
    }
}
