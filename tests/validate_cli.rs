use std::io::Write;
use std::process::Command;

fn wb_binary() -> std::path::PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("wb")
}

fn tmp_md(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new().suffix(".md").tempfile().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f
}

#[test]
fn wb_validate_clean_file_zero_exit() {
    let f = tmp_md("---\ntitle: OK\n---\n\n```bash\necho ok\n```\n");
    let output = Command::new(wb_binary())
        .args(["validate", f.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "clean file should exit 0");
}

#[test]
fn wb_validate_format_json_shape() {
    let f = tmp_md("---\ntitle: OK\n---\n\n```bash\necho ok\n```\n");
    let output = Command::new(wb_binary())
        .args(["validate", "--format", "json", f.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("expected JSON output, got: {stdout}\nerr: {e}"));
    assert!(v["diagnostics"].is_array(), "missing 'diagnostics' array");
    assert!(v["summary"]["errors"].is_number(), "missing summary.errors");
}

#[test]
fn wb_validate_strict_promotes_warnings() {
    // Out-of-range block number is a warning in normal mode → exit 0.
    // In strict mode → error → exit 3.
    let f =
        tmp_md("---\ntimeouts:\n  5: 30s\n---\n\n```bash\necho hi\n```\n```bash\necho bye\n```\n");
    let path = f.path().to_str().unwrap();

    let normal = Command::new(wb_binary())
        .args(["validate", path])
        .output()
        .unwrap();
    assert_eq!(
        normal.status.code(),
        Some(0),
        "warnings should not fail in normal mode"
    );

    let strict = Command::new(wb_binary())
        .args(["validate", "--strict", path])
        .output()
        .unwrap();
    assert_eq!(
        strict.status.code(),
        Some(3),
        "strict mode should exit 3 on warnings"
    );
}
