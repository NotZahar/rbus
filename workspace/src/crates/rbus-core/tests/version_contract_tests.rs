use std::path::{Path, PathBuf};

use rbus_core::constants::{
    CONTROL_PROTOCOL_VERSION, RBUS_API_VERSION, SHARED_MEMORY_ABI_VERSION, SHARED_MEMORY_MAGIC,
    STREAM_SEGMENT_ABI_VERSION, STREAM_SEGMENT_MAGIC, TOPIC_WAIT_ABI_VERSION,
};

const WORKSPACE_ROOT_ANCESTOR_INDEX: usize = 3;
const EXPECTED_API_VERSION: &str = "0.1.0";
const EXPECTED_CONTROL_PROTOCOL_VERSION: u16 = 2;
const EXPECTED_STREAM_SEGMENT_ABI_VERSION: u32 = 5;
const EXPECTED_TOPIC_WAIT_ABI_VERSION: u32 = 1;
const WORKSPACE_VERSION_LINE: &str = "version = \"0.1.0\"";
const API_VERSION_DOC_ROW: &str = "| Public API | `0.1.0` |";
const CONTROL_PROTOCOL_DOC_ROW: &str = "| Control protocol | `2` |";
const STREAM_SEGMENT_ABI_DOC_ROW: &str = "| Stream segment ABI | `5` |";
const TOPIC_WAIT_ABI_DOC_ROW: &str = "| Topic wait segment ABI | `1` |";
const README_VERSION_LINK_TARGET: &str = "(../docs/VERSIONS.ru.md)";
const CHANGELOG_STREAM_ABI_LINE: &str = "- Stream segment ABI version: `5`.";
const CHANGELOG_TOPIC_WAIT_ABI_LINE: &str = "- Topic wait segment ABI version: `1`.";

#[test]
fn api_and_abi_versions_are_centrally_synced() {
    assert_eq!(RBUS_API_VERSION, EXPECTED_API_VERSION);
    assert_eq!(CONTROL_PROTOCOL_VERSION, EXPECTED_CONTROL_PROTOCOL_VERSION);
    assert_eq!(
        STREAM_SEGMENT_ABI_VERSION,
        EXPECTED_STREAM_SEGMENT_ABI_VERSION
    );
    assert_eq!(TOPIC_WAIT_ABI_VERSION, EXPECTED_TOPIC_WAIT_ABI_VERSION);
    assert_eq!(SHARED_MEMORY_ABI_VERSION, STREAM_SEGMENT_ABI_VERSION);
    assert_eq!(SHARED_MEMORY_MAGIC, STREAM_SEGMENT_MAGIC);

    let workspace_root = workspace_root();
    let cargo_toml = read_workspace_file(&workspace_root, "src/Cargo.toml");
    let versions_document = read_workspace_file(&workspace_root, "docs/VERSIONS.ru.md");
    let readme = read_workspace_file(&workspace_root, "src/README.ru.md");
    let changelog = read_workspace_file(&workspace_root, "src/CHANGELOG.ru.md");

    assert_contains(&cargo_toml, WORKSPACE_VERSION_LINE);
    assert_contains(&versions_document, API_VERSION_DOC_ROW);
    assert_contains(&versions_document, CONTROL_PROTOCOL_DOC_ROW);
    assert_contains(&versions_document, STREAM_SEGMENT_ABI_DOC_ROW);
    assert_contains(&versions_document, TOPIC_WAIT_ABI_DOC_ROW);
    assert_contains(&readme, README_VERSION_LINK_TARGET);
    assert_contains(&changelog, CHANGELOG_STREAM_ABI_LINE);
    assert_contains(&changelog, CHANGELOG_TOPIC_WAIT_ABI_LINE);
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(WORKSPACE_ROOT_ANCESTOR_INDEX)
        .expect("workspace root should exist")
        .to_path_buf()
}

fn read_workspace_file(workspace_root: &Path, relative_path: &str) -> String {
    std::fs::read_to_string(workspace_root.join(relative_path)).expect("file should be readable")
}

fn assert_contains(contents: &str, expected: &str) {
    assert!(
        contents.contains(expected),
        "contents should contain {expected}"
    );
}
