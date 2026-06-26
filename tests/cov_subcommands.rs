//! Integration coverage for `wb`'s management / inspection subcommands.
//!
//! Every test spawns the real `wb` binary (so llvm-cov instruments it) with an
//! isolated `HOME` pointed at a fresh tempdir, so `~/.wb` state never leaks
//! between tests or onto the developer's machine. Inputs are written into
//! tempdirs or reused from the repo's `examples/` directory. Assertions check
//! structural invariants (diagnostic codes, JSON keys, exit codes) rather than
//! the presence of external tooling (Docker / Redis / doppler), so the suite is
//! deterministic on any host.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Locate the `wb` binary next to this test executable.
fn wb_binary() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("wb")
}

/// A fresh, isolated HOME for a test (so `~/.wb` is sandboxed).
fn temp_home() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

/// Output of one `wb` invocation: exit code (or -1), stdout, stderr.
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Run {
    fn out(&self) -> &str {
        &self.stdout
    }
}

/// Run `wb <args...>` with the given HOME, no stdin.
fn run_home(home: &std::path::Path, args: &[&str]) -> Run {
    run_full(home, args, None, &[])
}

/// Run `wb <args...>` with HOME + extra env vars + optional stdin payload.
fn run_full(
    home: &std::path::Path,
    args: &[&str],
    stdin: Option<&str>,
    envs: &[(&str, &str)],
) -> Run {
    let mut cmd = Command::new(wb_binary());
    cmd.args(args)
        .env("HOME", home)
        // Keep stderr quiet + deterministic regardless of the dev's env.
        .env("WB_LOG_LEVEL", "error")
        .env_remove("WB_CALLBACK_URL")
        .env_remove("WB_CALLBACK_SECRET")
        .env_remove("WB_CONFIG_PATH")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }
    let mut child = cmd.spawn().expect("spawn wb");
    if let Some(payload) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.as_bytes())
            .unwrap();
    }
    let out = child.wait_with_output().expect("wait wb");
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}

/// Write `content` to `<dir>/<name>` and return the path as a String.
fn write_file(dir: &std::path::Path, name: &str, content: &str) -> String {
    let p = dir.join(name);
    std::fs::write(&p, content).unwrap();
    p.to_string_lossy().to_string()
}

fn parse_json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("expected JSON, got error {e}:\n{s}"))
}

// ---------------------------------------------------------------------------
// inspect
// ---------------------------------------------------------------------------

#[test]
fn inspect_text_step_ids() {
    let home = temp_home();
    let r = run_home(home.path(), &["inspect", "examples/step-ids-demo.md"]);
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    // Human prose shows the explicit step id and the bash runtime.
    assert!(r.out().contains("health"), "missing step id: {}", r.out());
    assert!(r.out().contains("bash"));
}

#[test]
fn inspect_json_step_ids_structure() {
    let home = temp_home();
    let r = run_home(
        home.path(),
        &["inspect", "examples/step-ids-demo.md", "--json"],
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let v = parse_json(r.out());
    let blocks = v["blocks"].as_array().expect("blocks array");
    assert!(!blocks.is_empty());
    // First explicit-id block.
    assert_eq!(blocks[0]["step_id"], "health");
    assert_eq!(blocks[0]["language"], "bash");
    assert_eq!(blocks[0]["index"], 1);
    // At least one auto-derived id is present.
    let has_auto = blocks.iter().any(|b| {
        b["step_id"]
            .as_str()
            .map(|s| s.starts_with("auto-"))
            .unwrap_or(false)
    });
    assert!(has_auto, "expected an auto-<hash> step id");
    assert!(v["executable_count"].as_u64().unwrap() >= 1);
}

#[test]
fn inspect_text_includes_resolved_sandbox() {
    let home = temp_home();
    let r = run_home(home.path(), &["inspect", "examples/sandbox-demo.md"]);
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    // Resolved sandbox config + deterministic image tag are displayed.
    assert!(r.out().contains("sandbox: python"), "out: {}", r.out());
    assert!(
        r.out().contains("wb-sandbox:"),
        "missing image tag: {}",
        r.out()
    );
}

#[test]
fn inspect_json_resolved_sandbox() {
    let home = temp_home();
    let r = run_home(
        home.path(),
        &["inspect", "examples/sandbox-demo.md", "--json"],
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let v = parse_json(r.out());
    let sb = &v["frontmatter"]["sandbox"];
    assert_eq!(sb["kind"], "python");
    assert!(sb["image"].as_str().unwrap().starts_with("wb-sandbox:"));
    // apt/pip deps round-trip into the resolved view.
    assert!(sb["apt"].as_array().unwrap().iter().any(|x| x == "jq"));
    assert!(sb["pip"].as_array().unwrap().iter().any(|x| x == "httpx"));
}

#[test]
fn inspect_with_includes_splices_blocks() {
    let home = temp_home();
    let r = run_home(
        home.path(),
        &["inspect", "examples/include-demo.md", "--json"],
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let v = parse_json(r.out());
    // The included login workbook's block(s) splice in alongside the parent's.
    assert!(v["blocks"].as_array().unwrap().len() >= 2);
}

// ---------------------------------------------------------------------------
// validate
// ---------------------------------------------------------------------------

#[test]
fn validate_clean_text_and_json() {
    let home = temp_home();
    let r = run_home(home.path(), &["validate", "examples/hello.md"]);
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);

    let j = run_home(
        home.path(),
        &["validate", "examples/hello.md", "--format", "json"],
    );
    assert_eq!(j.code, 0);
    let v = parse_json(j.out());
    assert_eq!(v["summary"]["errors"], 0);
    assert_eq!(v["summary"]["warnings"], 0);
    assert!(v["diagnostics"].as_array().unwrap().is_empty());
}

#[test]
fn validate_duplicate_step_id() {
    let home = temp_home();
    let dir = temp_home();
    let f = write_file(
        dir.path(),
        "dup.md",
        "---\ntitle: dup\nruntime: bash\n---\n```bash {#same}\necho a\n```\n```bash {#same}\necho b\n```\n",
    );
    let r = run_home(home.path(), &["validate", &f, "--format", "json"]);
    assert_ne!(r.code, 0, "expected non-zero exit for duplicate id");
    let v = parse_json(r.out());
    let codes: Vec<&str> = v["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["code"].as_str())
        .collect();
    assert!(codes.contains(&"wb-step-001"), "codes: {codes:?}");
    assert_eq!(v["summary"]["errors"], 1);
}

#[test]
fn validate_malformed_expect() {
    let home = temp_home();
    let dir = temp_home();
    let f = write_file(
        dir.path(),
        "badexpect.md",
        "---\ntitle: bad\nruntime: bash\n---\n```bash\necho hi\n```\n```expect\nthis is not valid\n```\n",
    );
    let r = run_home(home.path(), &["validate", &f, "--format", "json"]);
    assert_ne!(r.code, 0);
    let v = parse_json(r.out());
    let codes: Vec<&str> = v["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["code"].as_str())
        .collect();
    assert!(codes.contains(&"wb-expect-001"), "codes: {codes:?}");
}

#[test]
fn validate_param_error() {
    let home = temp_home();
    let dir = temp_home();
    let f = write_file(
        dir.path(),
        "badparam.md",
        "---\ntitle: badparam\nruntime: bash\nparams:\n  region:\n    type: enum\n    one_of: [a, b]\n    default: zzz\n---\n```bash\necho $region\n```\n",
    );
    let r = run_home(home.path(), &["validate", &f]);
    assert_ne!(r.code, 0);
    assert!(r.stdout.contains("wb-param-001") || r.stderr.contains("wb-param-001"));
}

#[test]
fn validate_strict_flips_warning_to_error() {
    let home = temp_home();
    let dir = temp_home();
    // Fence attr + legacy frontmatter for the same block -> wb-step-002 warning.
    let f = write_file(
        dir.path(),
        "warn.md",
        "---\ntitle: warn\nruntime: bash\ntimeouts:\n  1: 30s\n---\n```bash {#a timeout=10s}\necho a\n```\n",
    );
    // Without --strict: warning only, exit 0.
    let lax = run_home(home.path(), &["validate", &f, "--format", "json"]);
    assert_eq!(lax.code, 0, "warning alone should not fail");
    let lv = parse_json(lax.out());
    assert_eq!(lv["summary"]["warnings"], 1);
    assert_eq!(lv["summary"]["errors"], 0);

    // With --strict: same diagnostic becomes an error, non-zero exit.
    let strict = run_home(
        home.path(),
        &["validate", &f, "--strict", "--format", "json"],
    );
    assert_ne!(strict.code, 0, "strict should fail on a warning");
    let sv = parse_json(strict.out());
    let codes: Vec<&str> = sv["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["code"].as_str())
        .collect();
    assert!(codes.contains(&"wb-step-002"), "codes: {codes:?}");
    assert_eq!(sv["summary"]["errors"], 1);
}

// ---------------------------------------------------------------------------
// test (expect/assert evaluation)
// ---------------------------------------------------------------------------

#[test]
fn test_passing_workbook_text_and_json() {
    let home = temp_home();
    let r = run_home(home.path(), &["test", "examples/test-demo.md"]);
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert!(r.out().contains("passed"));

    let j = run_home(
        home.path(),
        &["test", "examples/test-demo.md", "--format", "json"],
    );
    assert_eq!(j.code, 0);
    // stdout may carry block output before the JSON object; slice from first '{'.
    let start = j.out().find('{').expect("json object");
    let v = parse_json(&j.out()[start..]);
    assert_eq!(v["ok"], true);
    assert_eq!(v["failed"], 0);
    assert!(v["passed"].as_u64().unwrap() >= 1);
    assert!(v["files"].as_array().unwrap().len() == 1);
}

#[test]
fn test_failing_assertion_exits_one() {
    let home = temp_home();
    let dir = temp_home();
    let f = write_file(
        dir.path(),
        "fail.md",
        "---\nruntime: bash\n---\n```bash\necho hi\n```\n```expect\nstdout contains \"NOPE\"\n```\n",
    );
    let r = run_home(home.path(), &["test", &f, "--format", "json"]);
    assert_eq!(r.code, 1, "stderr: {}", r.stderr);
    let start = r.out().find('{').expect("json object");
    let v = parse_json(&r.out()[start..]);
    assert_eq!(v["ok"], false);
    assert_eq!(v["failed"], 1);
}

#[test]
fn test_folder_of_workbooks() {
    let home = temp_home();
    let dir = temp_home();
    write_file(
        dir.path(),
        "a.md",
        "---\nruntime: bash\n---\n```bash\necho a\n```\n```expect\nexit 0\n```\n",
    );
    write_file(
        dir.path(),
        "b.md",
        "---\nruntime: bash\n---\n```bash\necho b\n```\n```expect\nstdout contains \"b\"\n```\n",
    );
    let r = run_home(
        home.path(),
        &["test", &dir.path().to_string_lossy(), "--format", "json"],
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let start = r.out().find('{').expect("json object");
    let v = parse_json(&r.out()[start..]);
    assert_eq!(v["ok"], true);
    assert_eq!(v["files"].as_array().unwrap().len(), 2);
}

#[test]
fn test_no_expect_fences_exits_two() {
    let home = temp_home();
    let dir = temp_home();
    let f = write_file(
        dir.path(),
        "noexpect.md",
        "---\nruntime: bash\n---\n```bash\necho hi\n```\n",
    );
    let r = run_home(home.path(), &["test", &f]);
    assert_eq!(
        r.code, 2,
        "expected exit 2 when no assertions exist; stderr: {}",
        r.stderr
    );
}

// ---------------------------------------------------------------------------
// verify (docs-as-tests)
// ---------------------------------------------------------------------------

#[test]
fn verify_passing_doc_text_and_json() {
    let home = temp_home();
    let dir = temp_home();
    let f = write_file(dir.path(), "ok.md", "# Doc\n```bash\necho hi\n```\n");
    let r = run_home(home.path(), &["verify", &f]);
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);

    let j = run_home(home.path(), &["verify", &f, "--format", "json"]);
    assert_eq!(j.code, 0);
    let start = j.out().find('{').expect("json object");
    let v = parse_json(&j.out()[start..]);
    assert_eq!(v["ok"], true);
    assert_eq!(v["files"].as_array().unwrap()[0]["ok"], true);
}

#[test]
fn verify_failing_doc_exits_one() {
    let home = temp_home();
    let dir = temp_home();
    let f = write_file(dir.path(), "bad.md", "# Doc\n```bash\nexit 3\n```\n");
    let r = run_home(home.path(), &["verify", &f]);
    assert_eq!(r.code, 1, "stderr: {}", r.stderr);

    let j = run_home(home.path(), &["verify", &f, "--format", "json"]);
    assert_eq!(j.code, 1);
    let start = j.out().find('{').expect("json object");
    let v = parse_json(&j.out()[start..]);
    assert_eq!(v["ok"], false);
}

#[test]
fn verify_folder() {
    let home = temp_home();
    let dir = temp_home();
    write_file(dir.path(), "one.md", "# A\n```bash\necho one\n```\n");
    write_file(dir.path(), "two.md", "# B\n```bash\necho two\n```\n");
    let r = run_home(home.path(), &["verify", &dir.path().to_string_lossy()]);
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert!(r.out().contains("2/2") || r.out().contains("file(s) ok"));
}

// ---------------------------------------------------------------------------
// artifacts + runs (require a persisted run under a stable HOME)
// ---------------------------------------------------------------------------

/// Run examples/artifacts-demo.md under a stable HOME with a fixed run id so a
/// run dir + manifest exist. Block 1 writes orders.json with plain bash, so the
/// artifact is produced deterministically even though later (browser) blocks may
/// fail on a host without the browser runtime.
fn seed_artifacts_run(home: &std::path::Path) {
    let _ = run_full(
        home,
        &["run", "examples/artifacts-demo.md", "-q"],
        None,
        &[("WB_RECORDING_RUN_ID", "testrun")],
    );
}

#[test]
fn artifacts_list_text_and_json() {
    let home = temp_home();
    seed_artifacts_run(home.path());

    let r = run_home(home.path(), &["artifacts", "list", "--run", "testrun"]);
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert!(r.out().contains("orders.json"), "out: {}", r.out());

    let j = run_home(
        home.path(),
        &["artifacts", "list", "--run", "testrun", "--format", "json"],
    );
    assert_eq!(j.code, 0);
    let v = parse_json(j.out());
    assert_eq!(v["run_id"], "testrun");
    let arts = v["artifacts"].as_array().unwrap();
    let names: Vec<&str> = arts.iter().filter_map(|a| a["filename"].as_str()).collect();
    assert!(names.contains(&"orders.json"), "names: {names:?}");
    // Manifest carries provenance: a sha256 + the producing step id.
    let orders = arts
        .iter()
        .find(|a| a["filename"] == "orders.json")
        .unwrap();
    assert!(orders["sha256"].as_str().unwrap().len() == 64);
    assert!(orders["step_id"].as_str().unwrap().starts_with("auto-"));
}

#[test]
fn artifacts_open_prints_absolute_path() {
    let home = temp_home();
    seed_artifacts_run(home.path());
    let r = run_home(
        home.path(),
        &["artifacts", "open", "orders.json", "--run", "testrun"],
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let path = r.out().trim();
    assert!(path.ends_with("orders.json"));
    assert!(
        PathBuf::from(path).exists(),
        "open path should exist: {path}"
    );
}

#[test]
fn artifacts_export_copies_file() {
    let home = temp_home();
    seed_artifacts_run(home.path());
    let dst = temp_home();
    let target = dst.path().join("copied.json");
    let r = run_home(
        home.path(),
        &[
            "artifacts",
            "export",
            "orders.json",
            "--to",
            &target.to_string_lossy(),
            "--run",
            "testrun",
        ],
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    assert!(target.exists(), "exported file should exist");
    assert!(std::fs::read_to_string(&target).unwrap().contains("orders"));
}

#[test]
fn artifacts_missing_artifact_errors() {
    let home = temp_home();
    seed_artifacts_run(home.path());
    let r = run_home(
        home.path(),
        &["artifacts", "open", "nope.json", "--run", "testrun"],
    );
    assert_ne!(r.code, 0, "missing artifact should error");
    assert!(r.stderr.contains("not found") || r.stdout.contains("not found"));
}

#[test]
fn runs_list_and_show() {
    let home = temp_home();
    seed_artifacts_run(home.path());

    let list = run_home(home.path(), &["runs", "list"]);
    assert_eq!(list.code, 0, "stderr: {}", list.stderr);
    assert!(list.out().contains("testrun"));

    let list_json = run_home(home.path(), &["runs", "list", "--format", "json"]);
    assert_eq!(list_json.code, 0);
    let lv = parse_json(list_json.out());
    let ids: Vec<&str> = lv
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x["run_id"].as_str().or_else(|| x["id"].as_str()))
                .collect()
        })
        .unwrap_or_default();
    // Some builds wrap the list in an object; fall back to a substring check.
    assert!(ids.contains(&"testrun") || list_json.out().contains("testrun"));

    let show = run_home(
        home.path(),
        &["runs", "show", "testrun", "--format", "json"],
    );
    assert_eq!(show.code, 0, "stderr: {}", show.stderr);
    assert!(show.out().contains("testrun"));
}

#[test]
fn runs_show_missing_is_graceful() {
    let home = temp_home();
    // No run seeded; show should not panic and should mention the id.
    let r = run_home(home.path(), &["runs", "show", "ghost-run"]);
    assert!(r.code == 0 || r.code == 1, "unexpected exit {}", r.code);
    assert!(r.out().contains("ghost-run") || r.stderr.contains("ghost-run"));
}

// ---------------------------------------------------------------------------
// capture
// ---------------------------------------------------------------------------

#[test]
fn capture_assert_from_stdin_emits_runnable_workbook() {
    let home = temp_home();
    let dir = temp_home();
    let out = dir.path().join("captured.md");
    let r = run_full(
        home.path(),
        &["capture", "--assert", "-o", &out.to_string_lossy()],
        Some("echo hello\necho world\n"),
        &[],
    );
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let body = std::fs::read_to_string(&out).unwrap();
    // Runnable workbook: frontmatter + bash fences + asserted expect fences.
    assert!(body.contains("```bash"));
    assert!(body.contains("echo hello"));
    assert!(body.contains("echo world"));
    assert!(body.contains("```expect"));
    assert!(body.contains("exit 0"));

    // And the captured workbook actually passes when tested.
    let t = run_home(home.path(), &["test", &out.to_string_lossy()]);
    assert_eq!(t.code, 0, "captured workbook should pass: {}", t.stderr);
}

// ---------------------------------------------------------------------------
// config
// ---------------------------------------------------------------------------

#[test]
fn config_path_set_get_unset_lifecycle() {
    let home = temp_home();

    let path = run_home(home.path(), &["config", "path"]);
    assert_eq!(path.code, 0);
    assert!(path.out().contains("config.yaml"));

    let set = run_home(
        home.path(),
        &["config", "set", "callback.url", "https://example.com/hook"],
    );
    assert_eq!(set.code, 0, "stderr: {}", set.stderr);

    let get = run_home(home.path(), &["config", "get", "callback.url"]);
    assert_eq!(get.code, 0);
    assert!(get.out().contains("https://example.com/hook"));

    let get_json = run_home(
        home.path(),
        &["config", "get", "callback.url", "--format", "json"],
    );
    assert_eq!(get_json.code, 0);
    let v = parse_json(get_json.out());
    assert_eq!(v["key"], "callback.url");
    assert_eq!(v["value"], "https://example.com/hook");

    let unset = run_home(home.path(), &["config", "unset", "callback.url"]);
    assert_eq!(unset.code, 0);

    // get on an unset key: exit 2 but still emits {value:null} JSON.
    let after = run_home(
        home.path(),
        &["config", "get", "callback.url", "--format", "json"],
    );
    assert_eq!(after.code, 2, "unset key should exit 2");
    let av = parse_json(after.out());
    assert_eq!(av["key"], "callback.url");
    assert!(av["value"].is_null());
}

#[test]
fn config_list_json_known_keys() {
    let home = temp_home();
    let r = run_home(home.path(), &["config", "list", "--format", "json"]);
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    let v = parse_json(r.out());
    let keys: Vec<&str> = v["known_keys"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|k| k["key"].as_str())
        .collect();
    assert!(keys.contains(&"callback.url"), "keys: {keys:?}");
}

#[test]
fn config_set_unknown_key_rejected() {
    let home = temp_home();
    let r = run_home(home.path(), &["config", "set", "bogus.key", "val"]);
    assert_eq!(r.code, 2, "unknown key should be rejected with exit 2");
    assert!(r.stderr.contains("unknown config key") || r.stdout.contains("unknown config key"));
}

// ---------------------------------------------------------------------------
// version
// ---------------------------------------------------------------------------

#[test]
fn version_text_and_json() {
    let home = temp_home();
    let t = run_home(home.path(), &["version"]);
    assert_eq!(t.code, 0);
    assert!(t.out().to_lowercase().contains("wb") || t.out().contains('.'));

    let j = run_home(home.path(), &["version", "--format", "json"]);
    assert_eq!(j.code, 0);
    let v = parse_json(j.out());
    assert!(v["version"].as_str().unwrap().contains('.'));
}

// ---------------------------------------------------------------------------
// completion + man
// ---------------------------------------------------------------------------

#[test]
fn completion_scripts_for_each_shell() {
    let home = temp_home();
    for shell in ["bash", "zsh", "fish"] {
        let r = run_home(home.path(), &["completion", shell]);
        assert_eq!(r.code, 0, "{shell} completion failed: {}", r.stderr);
        assert!(!r.out().trim().is_empty(), "{shell} completion was empty");
        // Each script references the binary name.
        assert!(r.out().contains("wb"), "{shell} script missing 'wb'");
    }
}

#[test]
fn man_emits_roff() {
    let home = temp_home();
    let r = run_home(home.path(), &["man"]);
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
    // roff man page header macro.
    assert!(
        r.out().contains(".TH"),
        "man output not roff: {}",
        &r.out()[..r.out().len().min(80)]
    );
}

// ---------------------------------------------------------------------------
// transform (hidden helper)
// ---------------------------------------------------------------------------

#[test]
fn transform_help_and_basic_invoke() {
    let home = temp_home();
    let help = run_home(home.path(), &["transform", "--help"]);
    assert_eq!(help.code, 0);
    assert!(help.out().contains("transform") || help.out().contains("FILE"));

    // A file with no {{variables}} is a no-op but exits cleanly.
    let r = run_home(home.path(), &["transform", "examples/hello.md"]);
    assert_eq!(r.code, 0, "stderr: {}", r.stderr);
}

// ---------------------------------------------------------------------------
// pending + cancel
// ---------------------------------------------------------------------------

#[test]
fn pending_empty_text_json_no_reap() {
    let home = temp_home();
    let t = run_home(home.path(), &["pending"]);
    assert_eq!(t.code, 0);

    let j = run_home(home.path(), &["pending", "--format", "json"]);
    assert_eq!(j.code, 0);
    let v = parse_json(j.out());
    assert!(
        v.as_array().unwrap().is_empty(),
        "no runs seeded -> empty list"
    );

    let nr = run_home(home.path(), &["pending", "--no-reap"]);
    assert_eq!(nr.code, 0);
}

#[test]
fn cancel_unknown_id_is_graceful() {
    let home = temp_home();
    let r = run_home(home.path(), &["cancel", "nonexistent-xyz"]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.contains("no checkpoint") || r.stdout.contains("no checkpoint"));

    let j = run_home(
        home.path(),
        &["cancel", "nonexistent-xyz", "--format", "json"],
    );
    assert_eq!(j.code, 1);
    let v = parse_json(j.out());
    assert_eq!(v["ok"], false);
    assert_eq!(v["id"], "nonexistent-xyz");
}

#[test]
fn pause_then_cancel_real_run() {
    let home = temp_home();
    // Running the wait-demo pauses at the `wait` block and exits 42.
    let paused = run_home(
        home.path(),
        &[
            "run",
            "examples/wait-demo.md",
            "--checkpoint",
            "waitrun",
            "-q",
        ],
    );
    assert_eq!(
        paused.code, 42,
        "wait should pause with exit 42; stderr: {}",
        paused.stderr
    );

    // It now shows up in pending (read-only --no-reap so the timeout is left alone).
    let pend = run_home(home.path(), &["pending", "--no-reap", "--format", "json"]);
    assert_eq!(pend.code, 0);
    let pv = parse_json(pend.out());
    let ids: Vec<&str> = pv
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p["id"].as_str())
        .collect();
    assert!(ids.contains(&"waitrun"), "pending ids: {ids:?}");

    // Cancel drops it without resuming.
    let cancel = run_home(home.path(), &["cancel", "waitrun"]);
    assert_eq!(cancel.code, 0, "stderr: {}", cancel.stderr);
    let cancel_msg = format!("{}{}", cancel.stdout, cancel.stderr);
    assert!(cancel_msg.contains("waitrun") || cancel_msg.contains("cancelled"));

    // After cancel, pending is empty again.
    let after = run_home(home.path(), &["pending", "--format", "json"]);
    assert_eq!(after.code, 0);
    assert!(parse_json(after.out()).as_array().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// doctor
// ---------------------------------------------------------------------------

#[test]
fn doctor_text_runs_and_reports() {
    let home = temp_home();
    let r = run_home(home.path(), &["doctor"]);
    // doctor exits 0 when no checks fail; on a host missing a runtime it may be
    // non-zero, so accept either but require it to actually report a summary.
    assert!(
        r.code == 0 || r.code == 1,
        "unexpected doctor exit {}",
        r.code
    );
    assert!(
        r.out().contains("Pass:") || r.out().contains("wb"),
        "out: {}",
        r.out()
    );
}

#[test]
fn doctor_json_emits_checks() {
    let home = temp_home();
    let r = run_home(home.path(), &["doctor", "--format", "json"]);
    assert!(r.code == 0 || r.code == 1);
    let v = parse_json(r.out());
    let checks = v["checks"].as_array().expect("checks array");
    assert!(!checks.is_empty());
    // Every check has a name + status; don't assert docker/redis presence.
    for c in checks {
        assert!(c["name"].is_string());
        assert!(c["status"].is_string());
    }
    // The wb_version check is always present.
    assert!(checks.iter().any(|c| c["name"] == "wb_version"));
}

#[test]
fn doctor_deep_runs() {
    let home = temp_home();
    let r = run_home(home.path(), &["doctor", "--deep", "--format", "json"]);
    // Deep probes Docker/Redis/sidecar; presence is host-dependent, so only
    // require that it produced a structured set of checks without crashing.
    assert!(
        r.code == 0 || r.code == 1,
        "unexpected deep doctor exit {}",
        r.code
    );
    let v = parse_json(r.out());
    assert!(!v["checks"].as_array().unwrap().is_empty());
}
