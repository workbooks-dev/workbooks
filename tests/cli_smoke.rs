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
fn resume_rerun_and_goto_are_mutually_exclusive() {
    // F7b: --rerun-step and --goto-step are in the same clap group, so passing
    // both is a usage error (exit 2) before any checkpoint work.
    let output = Command::new(wb_binary())
        .args(["resume", "some-id", "--rerun-step", "a", "--goto-step", "b"])
        .output()
        .expect("failed to spawn wb");
    assert_eq!(
        output.status.code(),
        Some(2),
        "rerun-step + goto-step together should be a clap usage error"
    );
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
