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

fn write_workbook(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new()
        .suffix(".md")
        .tempfile()
        .expect("tempfile");
    f.write_all(content.as_bytes()).unwrap();
    f
}

#[test]
fn inspect_json_surfaces_step_ids() {
    // Two blocks: one with an explicit `{#login}` id, one without. The auto
    // id should start with `auto-` and the explicit id should be passed
    // through verbatim.
    let wb = write_workbook("```bash {#login}\necho first\n```\n\n```bash\necho second\n```\n");
    let out = Command::new(wb_binary())
        .args(["inspect", wb.path().to_str().unwrap(), "--json"])
        .output()
        .expect("failed to spawn wb");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let blocks = v["blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0]["step_id"], "login");
    let auto = blocks[1]["step_id"].as_str().unwrap();
    assert!(
        auto.starts_with("auto-"),
        "expected auto-derived id, got {auto}"
    );
}

#[test]
fn run_only_executes_just_the_selected_step() {
    // Three blocks; --only middle should run only the middle echo.
    let wb = write_workbook(
        "```bash\necho first\n```\n\n```bash {#middle}\necho second\n```\n\n```bash\necho third\n```\n",
    );
    let out = Command::new(wb_binary())
        .args(["run", wb.path().to_str().unwrap(), "--only", "middle"])
        .output()
        .expect("failed to spawn wb");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("second"),
        "selected step output missing:\n{combined}"
    );
    assert!(
        !combined.contains("first") || combined.contains("skipped"),
        "first block should be skipped, not executed:\n{combined}"
    );
    assert!(
        !combined.contains("third") || combined.contains("skipped"),
        "third block should be skipped, not executed:\n{combined}"
    );
}

#[test]
fn run_only_unknown_step_id_is_usage_error() {
    let wb = write_workbook("```bash {#a}\necho hi\n```\n");
    let out = Command::new(wb_binary())
        .args(["run", wb.path().to_str().unwrap(), "--only", "ghost"])
        .output()
        .expect("failed to spawn wb");
    // EXIT_USAGE = 2
    assert_eq!(
        out.status.code(),
        Some(2),
        "unknown step id should be a usage error"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("'ghost'") && stderr.contains("not found"),
        "stderr should name the missing id:\n{stderr}"
    );
}

#[test]
fn run_selection_with_checkpoint_is_refused() {
    let wb = write_workbook("```bash {#a}\necho hi\n```\n");
    let out = Command::new(wb_binary())
        .args([
            "run",
            wb.path().to_str().unwrap(),
            "--only",
            "a",
            "--checkpoint",
            "selection-conflict-test",
        ])
        .output()
        .expect("failed to spawn wb");
    assert_eq!(
        out.status.code(),
        Some(2),
        "selection + --checkpoint should be a usage error"
    );
}

#[test]
fn validate_flags_duplicate_explicit_step_ids() {
    let wb =
        write_workbook("```bash {#login}\necho first\n```\n\n```bash {#login}\necho second\n```\n");
    let out = Command::new(wb_binary())
        .args(["validate", wb.path().to_str().unwrap(), "--format", "json"])
        .output()
        .expect("failed to spawn wb");
    // wb-step-001 is an error → exit 3 (EXIT_WORKBOOK_INVALID).
    assert_eq!(out.status.code(), Some(3));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let codes: Vec<&str> = v["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["code"].as_str())
        .collect();
    assert!(
        codes.contains(&"wb-step-001"),
        "expected wb-step-001 in diagnostics: {codes:?}"
    );
}
