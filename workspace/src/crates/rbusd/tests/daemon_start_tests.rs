use std::process::Command;

#[test]
fn rbusd_starts_in_smoke_mode() {
    let binary_path = env!("CARGO_BIN_EXE_rbusd");
    let output = Command::new(binary_path)
        .args(["--domain", "test-domain", "--smoke"])
        .output()
        .expect("rbusd should run successfully");
    assert!(output.status.success(), "rbusd should run successfully");
}
