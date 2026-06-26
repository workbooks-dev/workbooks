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

// ─── wave 5: typed params + wb test ─────────────────────────────────────────

fn write_temp_md(slug: &str, body: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("wb-w5-{}-{}", slug, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{slug}.md"));
    std::fs::write(&path, body).unwrap();
    path
}

#[test]
fn run_param_injects_value_and_validates() {
    let md = write_temp_md(
        "param",
        "---\nruntime: bash\nparams:\n  greeting:\n    default: hi\n---\n```bash\necho \"g=$greeting\"\n```\n",
    );
    // Default injected.
    let out = Command::new(wb_binary())
        .args([md.to_str().unwrap()])
        .output()
        .expect("spawn wb");
    assert!(String::from_utf8_lossy(&out.stdout).contains("g=hi"));

    // Override via --param.
    let out = Command::new(wb_binary())
        .args([md.to_str().unwrap(), "--param", "greeting=yo"])
        .output()
        .expect("spawn wb");
    assert!(String::from_utf8_lossy(&out.stdout).contains("g=yo"));

    // Unknown param is a usage error (exit 2).
    let out = Command::new(wb_binary())
        .args([md.to_str().unwrap(), "--param", "nope=1"])
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(2));

    std::fs::remove_dir_all(md.parent().unwrap()).ok();
}

#[test]
fn run_missing_required_param_is_usage_error() {
    let md = write_temp_md(
        "reqparam",
        "---\nruntime: bash\nparams:\n  token:\n    required: true\n---\n```bash\necho ok\n```\n",
    );
    let out = Command::new(wb_binary())
        .args([md.to_str().unwrap(), "-q"])
        .output()
        .expect("spawn wb");
    assert_eq!(
        out.status.code(),
        Some(2),
        "missing required param should exit 2"
    );
    std::fs::remove_dir_all(md.parent().unwrap()).ok();
}

#[test]
fn test_command_passes_and_fails_by_assertions() {
    // All assertions pass → exit 0.
    let ok = write_temp_md(
        "testok",
        "---\nruntime: bash\n---\n```bash\necho ready\n```\n```expect\nexit 0\nstdout contains \"ready\"\n```\n",
    );
    let out = Command::new(wb_binary())
        .args(["test", ok.to_str().unwrap(), "-q"])
        .output()
        .expect("spawn wb");
    assert_eq!(
        out.status.code(),
        Some(0),
        "passing assertions should exit 0"
    );

    // A failing assertion → exit 1.
    let bad = write_temp_md(
        "testbad",
        "---\nruntime: bash\n---\n```bash\necho ready\n```\n```expect\nstdout contains \"absent\"\n```\n",
    );
    let out = Command::new(wb_binary())
        .args(["test", bad.to_str().unwrap(), "-q"])
        .output()
        .expect("spawn wb");
    assert_eq!(
        out.status.code(),
        Some(1),
        "failing assertion should exit 1"
    );

    std::fs::remove_dir_all(ok.parent().unwrap()).ok();
    std::fs::remove_dir_all(bad.parent().unwrap()).ok();
}

#[test]
fn test_command_no_assertions_is_usage_error() {
    let md = write_temp_md(
        "noassert",
        "---\nruntime: bash\n---\n```bash\necho hi\n```\n",
    );
    let out = Command::new(wb_binary())
        .args(["test", md.to_str().unwrap(), "-q"])
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(2), "no assertions should exit 2");
    std::fs::remove_dir_all(md.parent().unwrap()).ok();
}

#[test]
fn test_command_json_shape() {
    let md = write_temp_md(
        "testjson",
        "---\nruntime: bash\n---\n```bash\necho ok\n```\n```expect\nexit 0\n```\n",
    );
    let out = Command::new(wb_binary())
        .args(["test", md.to_str().unwrap(), "-q", "--format", "json"])
        .output()
        .expect("spawn wb");
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("test --format json should be valid JSON");
    assert_eq!(v["ok"], serde_json::Value::Bool(true));
    assert_eq!(v["passed"], serde_json::json!(1));
    assert!(v["files"].is_array());
    std::fs::remove_dir_all(md.parent().unwrap()).ok();
}

#[test]
fn dry_run_previews_without_executing() {
    let md = write_temp_md(
        "dryrun",
        "---\nruntime: bash\n---\n```bash\necho SHOULD_NOT_RUN\n```\n",
    );
    let out = Command::new(wb_binary())
        .args([md.to_str().unwrap(), "--dry-run"])
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("dry run:"), "missing plan header: {stdout}");
    assert!(
        stdout.contains("would run"),
        "missing plan summary: {stdout}"
    );
    // The block must NOT have executed.
    assert!(
        !stdout.contains("SHOULD_NOT_RUN"),
        "dry run must not execute blocks: {stdout}"
    );
    std::fs::remove_dir_all(md.parent().unwrap()).ok();
}

#[test]
fn tag_selection_runs_only_tagged_blocks() {
    let md = write_temp_md(
        "tagsel",
        "---\nruntime: bash\n---\n```bash {.a}\necho AAA\n```\n```bash {.b}\necho BBB\n```\n",
    );
    let out = Command::new(wb_binary())
        .args([md.to_str().unwrap(), "--tag", "a"])
        .output()
        .expect("spawn wb");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("AAA"), "tagged block should run: {stdout}");
    assert!(
        !stdout.contains("BBB"),
        "untagged block should be skipped: {stdout}"
    );

    // Unknown tag is a usage error.
    let out = Command::new(wb_binary())
        .args([md.to_str().unwrap(), "--tag", "ghost"])
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(2), "unknown tag should exit 2");
    std::fs::remove_dir_all(md.parent().unwrap()).ok();
}

#[test]
fn artifacts_manifest_list_and_open() {
    let dir = std::env::temp_dir().join(format!("wb-art-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let md = dir.join("a.md");
    std::fs::write(
        &md,
        "---\nruntime: bash\n---\n```bash {#gen}\necho hi > \"$WB_ARTIFACTS_DIR/out.txt\"\n```\n",
    )
    .unwrap();
    let run_id = format!("wb-test-art-{}", std::process::id());

    let run = Command::new(wb_binary())
        .args([md.to_str().unwrap(), "-q"])
        .env("WB_RECORDING_RUN_ID", &run_id)
        .output()
        .expect("spawn wb");
    assert_eq!(run.status.code(), Some(0));

    // list --format json should include the artifact with a checksum + step id.
    let list = Command::new(wb_binary())
        .args(["artifacts", "list", "--run", &run_id, "--format", "json"])
        .output()
        .expect("spawn wb");
    let v: serde_json::Value = serde_json::from_slice(&list.stdout).expect("json");
    let arts = v["artifacts"].as_array().expect("array");
    assert_eq!(arts.len(), 1, "one artifact expected: {v}");
    assert_eq!(arts[0]["filename"], "out.txt");
    assert_eq!(arts[0]["step_id"], "gen");
    assert!(arts[0]["sha256"].as_str().unwrap().len() == 64);

    // open prints the absolute path.
    let open = Command::new(wb_binary())
        .args(["artifacts", "open", "out.txt", "--run", &run_id])
        .output()
        .expect("spawn wb");
    assert_eq!(open.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&open.stdout)
        .trim()
        .ends_with("out.txt"));

    // Unknown artifact is an error.
    let missing = Command::new(wb_binary())
        .args(["artifacts", "open", "nope.txt", "--run", &run_id])
        .output()
        .expect("spawn wb");
    assert_eq!(missing.status.code(), Some(1));

    // runs show works.
    let show = Command::new(wb_binary())
        .args(["runs", "show", &run_id, "--format", "json"])
        .output()
        .expect("spawn wb");
    let sv: serde_json::Value = serde_json::from_slice(&show.stdout).expect("json");
    assert_eq!(sv["run_id"], run_id.as_str());
    assert_eq!(sv["artifacts"], serde_json::json!(1));

    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(format!(
        "{}/.wb/runs/{}",
        std::env::var("HOME").unwrap(),
        run_id
    ))
    .ok();
}
