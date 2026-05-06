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
