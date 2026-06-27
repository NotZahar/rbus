use rbus_ipc::memfd::create_memfd;
use rbus_ipc::mmap::MappedRegion;

#[test]
fn mmap_region_allows_shared_read_write() {
    let file_descriptor = create_memfd("rbus-mmap-test", 4096).expect("memfd should be created");
    let mut mapped_region =
        MappedRegion::map_shared(file_descriptor.as_raw_file_descriptor(), 4096, true)
            .expect("mmap should be created");
    mapped_region.as_mutable_slice()[0] = 17;
    assert_eq!(mapped_region.as_slice()[0], 17);
}
