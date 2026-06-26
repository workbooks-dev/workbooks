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

#[test]
fn cache_skips_unchanged_blocks() {
    let dir = std::env::temp_dir().join(format!("wb-cache-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let md = dir.join("c.md");
    std::fs::write(
        &md,
        "---\nruntime: bash\n---\n```bash\necho CACHE_ME\n```\n",
    )
    .unwrap();
    let cache_id = format!("wb-test-cache-{}", std::process::id());
    let cache_file = format!(
        "{}/.wb/cache/{}.json",
        std::env::var("HOME").unwrap(),
        cache_id
    );
    std::fs::remove_file(&cache_file).ok();

    // First run executes the block.
    let first = Command::new(wb_binary())
        .args([md.to_str().unwrap(), "--cache", &cache_id])
        .output()
        .expect("spawn wb");
    assert!(String::from_utf8_lossy(&first.stdout).contains("CACHE_ME"));

    // Second run skips it (cached); the output is no longer produced.
    let second = Command::new(wb_binary())
        .args([md.to_str().unwrap(), "--cache", &cache_id])
        .output()
        .expect("spawn wb");
    let out2 = format!(
        "{}{}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(
        !out2.contains("CACHE_ME"),
        "cached block should be skipped: {out2}"
    );
    assert!(out2.contains("skipped"), "should report a skip: {out2}");

    // --no-cache forces re-execution.
    let third = Command::new(wb_binary())
        .args([md.to_str().unwrap(), "--cache", &cache_id, "--no-cache"])
        .output()
        .expect("spawn wb");
    assert!(String::from_utf8_lossy(&third.stdout).contains("CACHE_ME"));

    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_file(&cache_file).ok();
}

#[test]
fn verify_passes_clean_doc_and_fails_broken_block() {
    // A doc whose blocks all succeed passes (assertions optional).
    let ok = write_temp_md(
        "verok",
        "---\nruntime: bash\n---\n# Doc\n```bash\necho works\n```\n",
    );
    let out = Command::new(wb_binary())
        .args(["verify", ok.to_str().unwrap(), "-q"])
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(0), "clean doc should verify");

    // A failing block fails verification.
    let bad = write_temp_md("verbad", "---\nruntime: bash\n---\n```bash\nfalse\n```\n");
    let out = Command::new(wb_binary())
        .args(["verify", bad.to_str().unwrap(), "-q"])
        .output()
        .expect("spawn wb");
    assert_eq!(
        out.status.code(),
        Some(1),
        "failing block should fail verify"
    );

    std::fs::remove_dir_all(ok.parent().unwrap()).ok();
    std::fs::remove_dir_all(bad.parent().unwrap()).ok();
}

#[test]
fn watch_once_and_json_snapshot() {
    let dir = std::env::temp_dir().join(format!("wb-watch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let md = dir.join("w.md");
    std::fs::write(&md, "---\nruntime: bash\n---\n```bash\necho one\n```\n").unwrap();
    let ckpt_id = format!("wb-test-watch-{}", std::process::id());
    let ckpt_file = format!(
        "{}/.wb/checkpoints/{}.json",
        std::env::var("HOME").unwrap(),
        ckpt_id
    );
    std::fs::remove_file(&ckpt_file).ok();

    // Produce a checkpoint.
    Command::new(wb_binary())
        .args([md.to_str().unwrap(), "--checkpoint", &ckpt_id, "-q"])
        .output()
        .expect("spawn wb");

    // --once snapshot.
    let once = Command::new(wb_binary())
        .args(["watch", &ckpt_id, "--once"])
        .output()
        .expect("spawn wb");
    assert_eq!(once.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&once.stdout).contains("watch"));

    // JSON snapshot.
    let js = Command::new(wb_binary())
        .args(["watch", &ckpt_id, "--format", "json"])
        .output()
        .expect("spawn wb");
    let v: serde_json::Value = serde_json::from_slice(&js.stdout).expect("json");
    assert_eq!(v["checkpoint"], ckpt_id.as_str());
    assert!(v["total_blocks"].as_u64().is_some());

    // Unknown checkpoint is a usage error.
    let missing = Command::new(wb_binary())
        .args(["watch", "definitely-not-a-real-ckpt-xyz", "--once"])
        .output()
        .expect("spawn wb");
    assert_eq!(missing.status.code(), Some(2));

    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_file(&ckpt_file).ok();
}

#[test]
fn changed_runs_only_edited_blocks() {
    let dir = std::env::temp_dir().join(format!("wb-changed-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let run_git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(&dir)
            .output()
            .expect("git");
    };
    run_git(&["init", "-q"]);
    run_git(&["config", "user.email", "t@t.co"]);
    run_git(&["config", "user.name", "t"]);
    let md = dir.join("flow.md");
    std::fs::write(
        &md,
        "---\nruntime: bash\n---\n```bash\necho AAA\n```\n```bash\necho BBB\n```\n",
    )
    .unwrap();
    run_git(&["add", "flow.md"]);
    run_git(&["commit", "-qm", "init"]);

    // Edit only the second block.
    std::fs::write(
        &md,
        "---\nruntime: bash\n---\n```bash\necho AAA\n```\n```bash\necho BBB_EDITED\n```\n",
    )
    .unwrap();

    let out = Command::new(wb_binary())
        .args([md.to_str().unwrap(), "--changed"])
        .current_dir(&dir)
        .output()
        .expect("spawn wb");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("BBB_EDITED"),
        "edited block should run: {stdout}"
    );
    assert!(
        !stdout.contains("AAA"),
        "unchanged block should be skipped: {stdout}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn capture_emits_runnable_workbook() {
    use std::io::Write;
    let dir = std::env::temp_dir().join(format!("wb-cap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("cap.md");

    let mut child = Command::new(wb_binary())
        .args([
            "capture",
            "--assert",
            "--title",
            "Cap",
            "-o",
            out.to_str().unwrap(),
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn wb");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"# Greet\necho captured-ok\n")
        .unwrap();
    let status = child.wait().expect("wait");
    assert!(status.success());

    let md = std::fs::read_to_string(&out).unwrap();
    assert!(md.contains("```bash"), "should emit a bash block: {md}");
    assert!(md.contains("echo captured-ok"));
    assert!(md.contains("stdout contains \"captured-ok\""));

    // The generated workbook must itself pass `wb test`.
    let test = Command::new(wb_binary())
        .args(["test", out.to_str().unwrap(), "-q"])
        .output()
        .expect("spawn wb");
    assert_eq!(
        test.status.code(),
        Some(0),
        "captured workbook should re-run green"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn require_trust_gates_untrusted_and_changed() {
    let dir = std::env::temp_dir().join(format!("wb-trust-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let md = dir.join("w.md");
    std::fs::write(&md, "---\nruntime: bash\n---\n```bash\necho ok\n```\n").unwrap();
    let trust_path = dir.join("trust.json");

    let run = |extra_trust: bool| {
        let mut c = Command::new(wb_binary());
        c.env("WB_TRUST_PATH", &trust_path);
        if extra_trust {
            c.args(["trust", "add", md.to_str().unwrap()]);
        } else {
            c.args([md.to_str().unwrap(), "--require-trust", "-q"]);
        }
        c.output().expect("spawn wb")
    };

    // Untrusted → refused (exit 2).
    assert_eq!(run(false).status.code(), Some(2));
    // Trust it.
    assert_eq!(run(true).status.code(), Some(0));
    // Now runs (exit 0).
    assert_eq!(run(false).status.code(), Some(0));
    // Edit → changed → refused again.
    std::fs::write(&md, "---\nruntime: bash\n---\n```bash\necho EDITED\n```\n").unwrap();
    assert_eq!(run(false).status.code(), Some(2));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn lock_and_run_locked_gate_drift() {
    let dir = std::env::temp_dir().join(format!("wb-lock-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let md = dir.join("w.md");
    std::fs::write(&md, "---\nruntime: bash\n---\n```bash\necho a\n```\n").unwrap();

    // Lock it.
    let lock = Command::new(wb_binary())
        .args(["lock", md.to_str().unwrap()])
        .output()
        .expect("spawn wb");
    assert_eq!(lock.status.code(), Some(0));
    assert!(dir.join("w.md.lock").exists());

    // Unchanged → --locked runs.
    let ok = Command::new(wb_binary())
        .args([md.to_str().unwrap(), "--locked", "-q"])
        .output()
        .expect("spawn wb");
    assert_eq!(ok.status.code(), Some(0));

    // Edit → --locked refuses (exit 2).
    std::fs::write(&md, "---\nruntime: bash\n---\n```bash\necho DRIFTED\n```\n").unwrap();
    let drift = Command::new(wb_binary())
        .args([md.to_str().unwrap(), "--locked", "-q"])
        .output()
        .expect("spawn wb");
    assert_eq!(drift.status.code(), Some(2));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn repair_unreachable_endpoint_aborts_safely() {
    // --repair with an unreachable endpoint must fail safe (abort): the failing
    // block bails and the next block does NOT run.
    let md = write_temp_md(
        "repair",
        "---\nruntime: bash\n---\n```bash\n( exit 1 )\n```\n```bash\necho SECOND_BLOCK\n```\n",
    );
    let out = Command::new(wb_binary())
        .args([
            md.to_str().unwrap(),
            "--bail",
            "--repair",
            "http://127.0.0.1:9", // discard port — refuses connections
            "--repair-max",
            "1",
        ])
        .output()
        .expect("spawn wb");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !combined.contains("SECOND_BLOCK"),
        "repair abort should bail before the second block: {combined}"
    );
    std::fs::remove_dir_all(md.parent().unwrap()).ok();
}

#[test]
fn sql_runtime_runs_sqlite_query() {
    // Skip gracefully when sqlite3 isn't on PATH (e.g. minimal CI).
    if Command::new("sqlite3").arg("--version").output().is_err() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("wb-sql-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("t.db");
    let md = dir.join("q.md");
    std::fs::write(
        &md,
        format!(
            "---\nruntime: bash\nenv:\n  WB_SQL_URL: {}\n---\n```sql\nCREATE TABLE t(x);\n```\n```sql\nINSERT INTO t VALUES ('hello-sql');\n```\n```sql\nSELECT x FROM t;\n```\n",
            db.display()
        ),
    )
    .unwrap();
    let out = Command::new(wb_binary())
        .args([md.to_str().unwrap()])
        .output()
        .expect("spawn wb");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("hello-sql"),
        "sql SELECT should return the row"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn events_jsonl_sink_writes_one_object_per_line() {
    let dir = std::env::temp_dir().join(format!("wb-events-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let md = dir.join("e.md");
    std::fs::write(&md, "---\nruntime: bash\n---\n```bash\necho a\n```\n").unwrap();
    let events = dir.join("events.jsonl");

    let out = Command::new(wb_binary())
        .args([
            md.to_str().unwrap(),
            "--events",
            events.to_str().unwrap(),
            "-q",
        ])
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(0));

    let body = std::fs::read_to_string(&events).expect("events file written");
    let lines: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(
        lines.len() >= 2,
        "expected step.complete + run.complete: {body}"
    );
    // Every line is a valid JSON object with an `event` field.
    let mut saw_complete = false;
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line).expect("each line is JSON");
        assert!(v.get("event").and_then(|e| e.as_str()).is_some());
        if v["event"] == "run.complete" {
            saw_complete = true;
        }
    }
    assert!(saw_complete, "should end with run.complete: {body}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn sign_verify_and_run_gate() {
    let dir = std::env::temp_dir().join(format!("wb-sign-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let keys = dir.join("keys");
    let md = dir.join("w.md");
    std::fs::write(&md, "---\nruntime: bash\n---\n```bash\necho ok\n```\n").unwrap();

    let with_env = |args: &[&str]| {
        Command::new(wb_binary())
            .args(args)
            .env("WB_KEYS_DIR", &keys)
            .output()
            .expect("spawn wb")
    };

    // --verify-sig before signing → refused (exit 2).
    assert_eq!(
        with_env(&[md.to_str().unwrap(), "--verify-sig", "-q"])
            .status
            .code(),
        Some(2)
    );
    // keygen + sign.
    assert_eq!(with_env(&["keygen"]).status.code(), Some(0));
    assert_eq!(
        with_env(&["sign", md.to_str().unwrap()]).status.code(),
        Some(0)
    );
    // verify-sig passes.
    assert_eq!(
        with_env(&["verify-sig", md.to_str().unwrap()])
            .status
            .code(),
        Some(0)
    );
    // run --verify-sig now succeeds.
    let run = with_env(&[md.to_str().unwrap(), "--verify-sig"]);
    assert!(String::from_utf8_lossy(&run.stdout).contains("ok"));
    // Tamper → verify-sig fails (exit 2).
    std::fs::write(
        &md,
        "---\nruntime: bash\n---\n```bash\necho TAMPERED\n```\n",
    )
    .unwrap();
    assert_eq!(
        with_env(&["verify-sig", md.to_str().unwrap()])
            .status
            .code(),
        Some(2)
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn allow_runtime_policy_gate() {
    let md = write_temp_md(
        "policy",
        "---\nruntime: bash\n---\n```bash\necho a\n```\n```python\nprint(1)\n```\n",
    );
    // Allowlist excludes python → refused (exit 2), nothing runs.
    let out = Command::new(wb_binary())
        .args([md.to_str().unwrap(), "--allow-runtime", "bash", "-q"])
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&out.stdout).contains("a"));
    // Allowlist includes both → runs.
    let out = Command::new(wb_binary())
        .args([
            md.to_str().unwrap(),
            "--allow-runtime",
            "bash",
            "--allow-runtime",
            "python",
        ])
        .output()
        .expect("spawn wb");
    assert!(String::from_utf8_lossy(&out.stdout).contains("a"));
    std::fs::remove_dir_all(md.parent().unwrap()).ok();
}
