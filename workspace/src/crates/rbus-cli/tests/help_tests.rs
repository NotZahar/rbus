use std::process::Command;

#[test]
fn cli_help_prints_usage() {
    let binary_path = env!("CARGO_BIN_EXE_rbus-cli");
    let output = Command::new(binary_path)
        .arg("--help")
        .output()
        .expect("rbus-cli should run successfully");
    assert!(output.status.success(), "operation should succeed");
    let stdout = String::from_utf8(output.stdout).expect("value should be utf8");
    assert!(stdout.contains("rbus-cli <command>"));
}
