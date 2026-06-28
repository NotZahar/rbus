use rbus_core::warning::WarningCode;
use rbus_ipc::memfd::create_memfd;
use rbus_ipc::mmap::MappedRegion;

const MMAP_TEST_LENGTH: usize = 4096;
const MMAP_SENTINEL_BYTE: u8 = 17;
const MMAP_HEADER_OFFSET: usize = 0;
const MISALIGNED_OFFSET: usize = 1;

#[repr(C, align(8))]
struct MappedTestHeader {
    value: u64,
}

#[test]
fn mmap_region_allows_shared_read_write() {
    let file_descriptor =
        create_memfd("rbus-mmap-test", MMAP_TEST_LENGTH).expect("memfd should be created");
    let mut mapped_region = MappedRegion::map_shared(
        file_descriptor.as_raw_file_descriptor(),
        MMAP_TEST_LENGTH,
        true,
    )
    .expect("mmap should be created");
    mapped_region.as_mutable_slice()[MMAP_HEADER_OFFSET] = MMAP_SENTINEL_BYTE;
    assert_eq!(
        mapped_region.as_slice()[MMAP_HEADER_OFFSET],
        MMAP_SENTINEL_BYTE
    );
}

#[test]
fn mmap_region_checks_typed_projection_bounds_and_alignment() {
    let file_descriptor = create_memfd("rbus-mmap-projection-test", MMAP_TEST_LENGTH)
        .expect("memfd should be created");
    let mapped_region = MappedRegion::map_shared(
        file_descriptor.as_raw_file_descriptor(),
        MMAP_TEST_LENGTH,
        true,
    )
    .expect("mmap should be created");

    let header_pointer = mapped_region
        .typed_pointer::<MappedTestHeader>(MMAP_HEADER_OFFSET, "mmap_projection_ok")
        .expect("typed projection should be valid");
    let bounds_warning = mapped_region
        .typed_pointer::<MappedTestHeader>(MMAP_TEST_LENGTH, "mmap_projection_bounds")
        .expect_err("projection beyond the mapping should return warning");
    let alignment_warning = mapped_region
        .typed_pointer::<MappedTestHeader>(MISALIGNED_OFFSET, "mmap_projection_alignment")
        .expect_err("misaligned projection should return warning");

    assert_eq!(
        header_pointer.as_ptr() as usize % core::mem::align_of::<MappedTestHeader>(),
        0
    );
    assert_eq!(bounds_warning.code(), WarningCode::SegmentAbiMismatch);
    assert_eq!(alignment_warning.code(), WarningCode::SegmentAbiMismatch);
}
