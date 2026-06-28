use core::sync::atomic::{AtomicU32, Ordering};
use std::os::fd::RawFd;
use std::path::PathBuf;

use rbus_core::ids::{PublisherId, StreamId, TopicHash};
use rbus_core::warning::WarningCode;
use rbus_ipc::control::{FrameHeader, decode_frame_header};
use rbus_ipc::futex::{futex_wait_private, futex_wake_private};
use rbus_ipc::mmap::MappedRegion;
use rbus_ipc::permissions::ensure_private_directory;
use rbus_ipc::stream_segment::{
    create_stream_segment, map_stream_segment, snapshot_stream_segment,
};

const UNKNOWN_OPCODE: u16 = u16::MAX;
const TEST_PIPE_LENGTH: usize = 4096;
const TEST_PIPE_BYTE: u8 = 7;
const PIPE_READ_INDEX: usize = 0;
const PIPE_WRITE_INDEX: usize = 1;
const PIPE_FD_COUNT: usize = 2;
const INVALID_FILE_DESCRIPTOR: RawFd = -1;
const SUCCESS_RETURN_CODE: i32 = 0;
const PIPE_BYTE_COUNT: usize = 1;
const TEST_SLOT_SIZE: usize = 64;
const TEST_DEPTH: usize = 8;
const ABI_MISMATCH_SIZE_DELTA: usize = 64;

#[test]
fn protocol_mismatch_warning_path_rejects_unknown_opcode() {
    let mut frame_header = [0_u8; FrameHeader::ENCODED_SIZE];
    frame_header[0..2].copy_from_slice(&UNKNOWN_OPCODE.to_le_bytes());
    let original_header = frame_header;

    let warning = decode_frame_header(&frame_header).expect_err("operation should return warning");

    assert_eq!(warning.code(), WarningCode::ProtocolMismatch);
    assert_eq!(frame_header, original_header);
}

#[test]
fn permission_denied_warning_path_preserves_blocking_file() {
    let file_path = unique_temp_path("permission-file");
    std::fs::write(&file_path, b"not-a-directory").expect("test file should be created");

    let warning =
        ensure_private_directory(&file_path).expect_err("operation should return warning");

    assert_eq!(warning.code(), WarningCode::PermissionDenied);
    assert!(file_path.is_file());
    std::fs::remove_file(file_path).expect("test file should be removed");
}

#[test]
fn syscall_failed_warning_path_leaves_futex_state_unchanged() {
    let futex_word = AtomicU32::new(1);

    let warning = futex_wait_private(&futex_word, 0).expect_err("operation should return warning");

    assert_eq!(warning.code(), WarningCode::SyscallFailed);
    assert_eq!(futex_word.load(Ordering::Relaxed), 1);
}

#[test]
fn resource_limit_warning_path_preserves_futex_state() {
    let futex_word = AtomicU32::new(1);

    let warning =
        futex_wake_private(&futex_word, u32::MAX).expect_err("operation should return warning");

    assert_eq!(warning.code(), WarningCode::ResourceLimit);
    assert_eq!(futex_word.load(Ordering::Relaxed), 1);
}

#[test]
fn mmap_failed_warning_path_keeps_pipe_usable() {
    let pipe = Pipe::new();

    let warning = MappedRegion::map_shared(
        pipe.file_descriptors[PIPE_READ_INDEX],
        TEST_PIPE_LENGTH,
        true,
    )
    .expect_err("operation should return warning");
    let written = write_pipe_byte(pipe.file_descriptors[PIPE_WRITE_INDEX]);

    assert_eq!(warning.code(), WarningCode::MmapFailed);
    assert_eq!(written, PIPE_BYTE_COUNT as isize);
}

#[test]
fn segment_abi_mismatch_warning_path_preserves_original_segment() {
    let created_segment = create_stream_segment(
        PublisherId::new(1),
        StreamId::new(2),
        TopicHash::new(3),
        TEST_SLOT_SIZE,
        TEST_DEPTH,
    )
    .expect("stream segment should be created");
    let mut mismatched_metadata = created_segment.metadata;
    mismatched_metadata.segment_size = mismatched_metadata
        .segment_size
        .saturating_add(ABI_MISMATCH_SIZE_DELTA);

    let warning = map_stream_segment(&created_segment.file_descriptor, mismatched_metadata, true)
        .expect_err("operation should return warning");
    let snapshot = snapshot_stream_segment(
        &created_segment.file_descriptor,
        created_segment.metadata,
        0,
    )
    .expect("original segment should remain valid");

    assert_eq!(warning.code(), WarningCode::SegmentAbiMismatch);
    assert_eq!(warning.stream_id(), Some(StreamId::new(2)));
    assert_eq!(snapshot.metadata, created_segment.metadata);
    assert_eq!(snapshot.stats.warnings_total, 0);
}

fn unique_temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("rbus-ipc-{label}-{}", std::process::id()))
}

fn write_pipe_byte(file_descriptor: RawFd) -> isize {
    let buffer = [TEST_PIPE_BYTE];
    // SAFETY: The descriptor is owned by Pipe and the buffer is valid for PIPE_BYTE_COUNT bytes.
    unsafe {
        libc::write(
            file_descriptor,
            buffer.as_ptr().cast::<libc::c_void>(),
            PIPE_BYTE_COUNT,
        )
    }
}

struct Pipe {
    file_descriptors: [RawFd; PIPE_FD_COUNT],
}

impl Pipe {
    fn new() -> Self {
        let mut file_descriptors = [INVALID_FILE_DESCRIPTOR; PIPE_FD_COUNT];
        // SAFETY: pipe writes exactly two file descriptors into the provided array on success.
        let result = unsafe { libc::pipe(file_descriptors.as_mut_ptr()) };
        assert_eq!(result, SUCCESS_RETURN_CODE, "pipe should be created");
        Self { file_descriptors }
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        for file_descriptor in self.file_descriptors {
            if file_descriptor >= 0 {
                // SAFETY: Each descriptor is created by pipe and closed exactly once in Drop.
                unsafe {
                    libc::close(file_descriptor);
                }
            }
        }
    }
}
