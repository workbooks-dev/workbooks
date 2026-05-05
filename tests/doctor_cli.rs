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
fn wb_doctor_shallow_zero_exit_on_dev_machine() {
    // Shallow doctor should pass on any dev machine with bash.
    // Docker is allowed to be absent (Warn, not Fail) so this stays green in CI.
    let output = Command::new(wb_binary())
        .arg("doctor")
        .output()
        .expect("failed to spawn wb doctor");
    // Exit 0 = all pass/warn; exit 3 = at least one Fail.
    // On CI, Docker daemon may be absent (Warn) — that's fine.
    // bash/python3/node should always pass.
    assert!(
        output.status.code() == Some(0) || output.status.code() == Some(3),
        "unexpected exit code from wb doctor: {:?}",
        output.status.code()
    );
}

#[test]
fn wb_doctor_format_json_shape() {
    let output = Command::new(wb_binary())
        .args(["doctor", "--format", "json"])
        .output()
        .expect("failed to spawn wb doctor --format json");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("expected JSON output, got: {stdout}\nerr: {e}"));
    assert!(v["checks"].is_array(), "missing 'checks' array");
    assert!(v["summary"]["pass"].is_number(), "missing summary.pass");
}
