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

#[test]
fn completion_subcommand_emits_script() {
    let output = Command::new(wb_binary())
        .args(["completion", "bash"])
        .output()
        .expect("failed to spawn wb");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("_wb"),
        "expected a bash completion function"
    );
}

#[test]
fn version_format_json_is_parseable() {
    let output = Command::new(wb_binary())
        .args(["version", "--format", "json"])
        .output()
        .expect("failed to spawn wb");
    assert_eq!(output.status.code(), Some(0));
    let v: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("version --format json should be valid JSON");
    assert!(v.get("version").and_then(|x| x.as_str()).is_some());
}

#[test]
fn bad_format_is_usage_error() {
    let output = Command::new(wb_binary())
        .args(["version", "--format", "xml"])
        .output()
        .expect("failed to spawn wb");
    assert_eq!(
        output.status.code(),
        Some(2),
        "invalid --format should exit 2"
    );
}

#[test]
fn config_list_format_json_is_parseable() {
    let dir = std::env::temp_dir().join(format!("wb-cfg-json-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg_path = dir.join("config.yaml");

    let out = Command::new(wb_binary())
        .args(["config", "list", "--format", "json"])
        .env("WB_CONFIG_PATH", &cfg_path)
        .output()
        .expect("failed to spawn wb");
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)
        .expect("config list --format json should be valid JSON");
    assert!(v.get("values").is_some());
    assert!(v.get("known_keys").and_then(|k| k.as_array()).is_some());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn config_set_get_roundtrip_via_cli() {
    let dir = std::env::temp_dir().join(format!("wb-cfg-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg_path = dir.join("config.yaml");

    let set = Command::new(wb_binary())
        .args(["config", "set", "callback.url", "https://x/wb"])
        .env("WB_CONFIG_PATH", &cfg_path)
        .output()
        .expect("failed to spawn wb");
    assert_eq!(set.status.code(), Some(0), "set should succeed");

    let get = Command::new(wb_binary())
        .args(["config", "get", "callback.url"])
        .env("WB_CONFIG_PATH", &cfg_path)
        .output()
        .expect("failed to spawn wb");
    assert_eq!(get.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&get.stdout).trim(), "https://x/wb");

    // Unknown key is a usage error.
    let bad = Command::new(wb_binary())
        .args(["config", "set", "callback.bogus", "x"])
        .env("WB_CONFIG_PATH", &cfg_path)
        .output()
        .expect("failed to spawn wb");
    assert_eq!(bad.status.code(), Some(2), "unknown key should exit 2");

    std::fs::remove_dir_all(&dir).ok();
}
