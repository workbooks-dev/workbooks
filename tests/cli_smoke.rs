use std::process::Command;

fn wb_binary() -> std::path::PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("wb")
}

#[test]
fn unknown_flag_exits_2() {
    // An unrecognised flag (not a subcommand and not a file) produces a clap
    // usage error with exit code 2.
    let output = Command::new(wb_binary())
        .arg("--nonsense-flag-xyz")
        .output()
        .expect("failed to spawn wb");
    assert_eq!(output.status.code(), Some(2), "unknown flag should exit 2");
}

#[test]
fn version_subcommand_prints_version() {
    let output = Command::new(wb_binary())
        .arg("version")
        .output()
        .expect("failed to spawn wb");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("wb v"),
        "expected version output starting with 'wb v', got: {stdout}"
    );
}
