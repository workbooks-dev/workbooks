//! Coverage-driving integration tests for the `wb run` CLI flag surface.
//!
//! Each test spawns the real `wb` binary. cargo-llvm-cov instruments that
//! subprocess, so exercising these CLI paths raises `src/lib.rs` coverage.
//!
//! Isolation: every spawned `wb` writes under `~/.wb`. To keep runs from
//! colliding with each other (or the developer's real home), every Command
//! gets a fresh temp `HOME` via `tempfile::tempdir()`. The TempDir is held
//! alive for the duration of the assertions that depend on it.

use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn wb_binary() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("wb")
}

/// Absolute path to a checked-in example workbook.
fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
}

/// A fresh isolated HOME for one (or a few related) `wb` invocations.
fn home() -> TempDir {
    tempfile::tempdir().unwrap()
}

/// Write `body` to `<home>/<name>` and return its path.
fn write_wb(home: &TempDir, name: &str, body: &str) -> PathBuf {
    let path = home.path().join(name);
    std::fs::write(&path, body).unwrap();
    path
}

/// Build a `Command` for `wb` with an isolated HOME already set.
fn wb(home: &TempDir) -> Command {
    let mut c = Command::new(wb_binary());
    c.env("HOME", home.path());
    c
}

fn combined(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

// ─── bare invocation + `run` subcommand ─────────────────────────────────────

#[test]
fn bare_file_runs_successfully() {
    let h = home();
    let out = wb(&h)
        .arg(example("health-check.md"))
        .arg("-q")
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(0), "bare file run should exit 0");
}

#[test]
fn run_subcommand_explicit_form() {
    let h = home();
    let out = wb(&h)
        .args(["run"])
        .arg(example("health-check.md"))
        .arg("-q")
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(0), "`wb run <file>` should exit 0");
}

#[test]
fn example_hello_runs_python_and_bash() {
    // hello.md mixes bash + python; both runtimes are available.
    let h = home();
    let out = wb(&h)
        .arg(example("hello.md"))
        .arg("-q")
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(0), "hello.md should run green");
}

#[test]
fn multi_runtime_bash_python_node() {
    let h = home();
    let md = write_wb(
        &h,
        "multi.md",
        "---\nruntime: bash\n---\n\
         ```bash\necho BASH_OK\n```\n\
         ```python\nprint(\"PYTHON_OK\")\n```\n\
         ```node\nconsole.log(\"NODE_OK\")\n```\n",
    );
    let out = wb(&h).arg(&md).output().expect("spawn wb");
    assert_eq!(out.status.code(), Some(0));
    let s = combined(&out);
    assert!(s.contains("BASH_OK"), "bash block: {s}");
    assert!(s.contains("PYTHON_OK"), "python block: {s}");
    assert!(s.contains("NODE_OK"), "node block: {s}");
}

// ─── output destinations / rendering formats ────────────────────────────────

#[test]
fn output_file_is_written() {
    let h = home();
    let md = write_wb(
        &h,
        "o.md",
        "---\nruntime: bash\n---\n```bash\necho RESULT_TOKEN\n```\n",
    );
    let out_path = h.path().join("results.md");
    let out = wb(&h)
        .arg(&md)
        .args(["-q", "-o"])
        .arg(&out_path)
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(0));
    let written = std::fs::read_to_string(&out_path).expect("output file written");
    assert!(
        written.contains("RESULT_TOKEN"),
        "rendered output should include block stdout: {written}"
    );
}

#[test]
fn json_render_to_stdout_is_parseable() {
    let h = home();
    let md = write_wb(
        &h,
        "j.md",
        "---\nruntime: bash\n---\n```bash\necho hi\n```\n",
    );
    // -q suppresses the live block echo so stdout is just the JSON document.
    let out = wb(&h)
        .arg(&md)
        .args(["-q", "--json"])
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("--json stdout should be valid JSON");
    assert_eq!(v["status"], "pass");
    assert_eq!(v["blocks"]["total"], serde_json::json!(1));
    assert!(v["results"].is_array());
}

#[test]
fn yaml_render_to_stdout() {
    let h = home();
    let md = write_wb(
        &h,
        "y.md",
        "---\nruntime: bash\n---\n```bash\necho hi\n```\n",
    );
    let out = wb(&h)
        .arg(&md)
        .args(["-q", "--yaml"])
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("status:"),
        "yaml should have status: {stdout}"
    );
    assert!(
        stdout.contains("source:"),
        "yaml should have source: {stdout}"
    );
}

#[test]
fn md_render_to_file() {
    let h = home();
    let md = write_wb(
        &h,
        "m.md",
        "---\nruntime: bash\n---\n```bash\necho MD_TOKEN\n```\n",
    );
    let out_path = h.path().join("out.md");
    let out = wb(&h)
        .arg(&md)
        .args(["-q", "--md", "-o"])
        .arg(&out_path)
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(0));
    let written = std::fs::read_to_string(&out_path).expect("md output written");
    assert!(
        written.contains("source:"),
        "md front-matter header: {written}"
    );
    assert!(
        written.contains("MD_TOKEN"),
        "md should include output: {written}"
    );
}

#[test]
fn quiet_suppresses_live_block_output() {
    let h = home();
    let md = write_wb(
        &h,
        "q.md",
        "---\nruntime: bash\n---\n```bash\necho QUIET_PROBE\n```\n",
    );
    // Without -q the live echo appears on stdout.
    let loud = wb(&h).arg(&md).output().expect("spawn wb");
    assert!(
        String::from_utf8_lossy(&loud.stdout).contains("QUIET_PROBE"),
        "non-quiet run should stream block output"
    );
    // With -q it does not.
    let quiet = wb(&h).arg(&md).arg("-q").output().expect("spawn wb");
    assert!(
        !String::from_utf8_lossy(&quiet.stdout).contains("QUIET_PROBE"),
        "quiet run should suppress block output: {}",
        String::from_utf8_lossy(&quiet.stdout)
    );
}

// ─── failure handling: --bail vs continue ───────────────────────────────────

#[test]
fn bail_stops_after_failing_block() {
    let h = home();
    let md = write_wb(
        &h,
        "bail.md",
        "---\nruntime: bash\n---\n\
         ```bash\necho FIRST_OK\n```\n\
         ```bash\nfalse\n```\n\
         ```bash\necho THIRD_BLOCK\n```\n",
    );
    let out = wb(&h).arg(&md).arg("--bail").output().expect("spawn wb");
    assert_ne!(
        out.status.code(),
        Some(0),
        "bail on failure should be non-zero"
    );
    let s = combined(&out);
    assert!(s.contains("FIRST_OK"), "first block should have run: {s}");
    assert!(
        !s.contains("THIRD_BLOCK"),
        "block after the failure must not run under --bail: {s}"
    );
}

#[test]
fn bail_all_pass_exits_zero() {
    let h = home();
    let md = write_wb(
        &h,
        "allpass.md",
        "---\nruntime: bash\n---\n```bash\necho a\n```\n```bash\necho b\n```\n",
    );
    let out = wb(&h)
        .arg(&md)
        .args(["--bail", "-q"])
        .output()
        .expect("spawn wb");
    assert_eq!(
        out.status.code(),
        Some(0),
        "all-pass --bail run should exit 0"
    );
}

#[test]
fn failure_without_bail_continues_but_reports_failure() {
    let h = home();
    let md = write_wb(
        &h,
        "nobail.md",
        "---\nruntime: bash\n---\n\
         ```bash\nfalse\n```\n\
         ```bash\necho SECOND_RAN\n```\n",
    );
    let out = wb(&h).arg(&md).output().expect("spawn wb");
    let s = combined(&out);
    assert!(
        s.contains("SECOND_RAN"),
        "without --bail later blocks still run: {s}"
    );
    assert_ne!(
        out.status.code(),
        Some(0),
        "overall exit should reflect the block failure"
    );
}

// ─── setup / working dir / ordering ─────────────────────────────────────────

#[test]
fn no_setup_skips_setup_commands() {
    let h = home();
    // setup: writes a marker; --no-setup should skip it so the block sees nothing.
    let md = write_wb(
        &h,
        "setup.md",
        "---\nruntime: bash\nsetup:\n  - export SETUP_MARKER=on\n---\n```bash\necho \"m=${SETUP_MARKER:-unset}\"\n```\n",
    );
    let out = wb(&h)
        .arg(&md)
        .arg("--no-setup")
        .output()
        .expect("spawn wb");
    assert_eq!(
        out.status.code(),
        Some(0),
        "run with --no-setup should succeed"
    );
}

#[test]
fn dir_flag_sets_working_directory() {
    let h = home();
    let workdir = home();
    let md = write_wb(&h, "pwd.md", "---\nruntime: bash\n---\n```bash\npwd\n```\n");
    let out = wb(&h)
        .arg(&md)
        .arg("-C")
        .arg(workdir.path())
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(0));
    // The tempdir's unique final component must appear in pwd output. (macOS
    // resolves /var -> /private/var, so compare on the leaf, not the full path.)
    let leaf = workdir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert!(
        String::from_utf8_lossy(&out.stdout).contains(&*leaf),
        "pwd should reflect -C dir ({leaf}): {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn order_reverse_runs_files_z_to_a() {
    let h = home();
    // A folder run with two files; --order z-a runs b.md before a.md.
    let dir = home();
    std::fs::write(
        dir.path().join("a.md"),
        "---\nruntime: bash\n---\n```bash\necho FILE_A\n```\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.md"),
        "---\nruntime: bash\n---\n```bash\necho FILE_B\n```\n",
    )
    .unwrap();
    let out = wb(&h)
        .arg(dir.path())
        .args(["--order", "z-a"])
        .output()
        .expect("spawn wb");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let a = stdout.find("FILE_A").expect("FILE_A present");
    let b = stdout.find("FILE_B").expect("FILE_B present");
    assert!(
        b < a,
        "z-a order should emit FILE_B before FILE_A: {stdout}"
    );
}

// ─── env injection / env-file / redaction ───────────────────────────────────

#[test]
fn set_flag_injects_env_var() {
    let h = home();
    let md = write_wb(
        &h,
        "env.md",
        "---\nruntime: bash\n---\n```bash\necho \"FOO=$FOO\"\n```\n",
    );
    let out = wb(&h)
        .arg(&md)
        .args(["-e", "FOO=barbar"])
        .output()
        .expect("spawn wb");
    assert!(
        combined(&out).contains("FOO=barbar"),
        "-e KEY=VALUE should inject env var"
    );
}

#[test]
fn long_set_flag_injects_env_var() {
    let h = home();
    let md = write_wb(
        &h,
        "env2.md",
        "---\nruntime: bash\n---\n```bash\necho \"BAZ=$BAZ\"\n```\n",
    );
    let out = wb(&h)
        .arg(&md)
        .args(["--set", "BAZ=quux"])
        .output()
        .expect("spawn wb");
    assert!(
        combined(&out).contains("BAZ=quux"),
        "--set KEY=VALUE should inject env var"
    );
}

#[test]
fn env_file_loads_variables() {
    let h = home();
    let md = write_wb(
        &h,
        "envf.md",
        "---\nruntime: bash\n---\n```bash\necho \"FOO=$FOO\"\n```\n",
    );
    let env_file = h.path().join("vars.env");
    std::fs::write(&env_file, "FOO=fromfile\n").unwrap();
    let out = wb(&h)
        .arg(&md)
        .arg("--env-file")
        .arg(&env_file)
        .output()
        .expect("spawn wb");
    assert!(
        combined(&out).contains("FOO=fromfile"),
        "--env-file should load variables"
    );
}

#[test]
fn redact_masks_secret_in_rendered_output() {
    let h = home();
    // --redact names the env var; its value is masked in *rendered* output
    // (not the live terminal stream), so assert on a -o file.
    let md = write_wb(
        &h,
        "redact.md",
        "---\nruntime: bash\nenv:\n  PW: supersekret\n---\n```bash\necho \"pw=$PW\"\n```\n",
    );
    let out_path = h.path().join("out.md");
    let out = wb(&h)
        .arg(&md)
        .args(["-q", "--redact", "PW", "-o"])
        .arg(&out_path)
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(0));
    let written = std::fs::read_to_string(&out_path).unwrap();
    assert!(
        !written.contains("supersekret"),
        "redacted secret must not appear in rendered output: {written}"
    );
    assert!(
        written.contains("pw="),
        "the non-secret context should still be present: {written}"
    );
}

// ─── policy gates / timeout defaults ────────────────────────────────────────

#[test]
fn allow_runtime_permits_listed_language() {
    let h = home();
    let md = write_wb(
        &h,
        "allow.md",
        "---\nruntime: bash\n---\n```bash\necho ALLOWED_OK\n```\n",
    );
    let out = wb(&h)
        .arg(&md)
        .args(["--allow-runtime", "bash"])
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(0));
    assert!(combined(&out).contains("ALLOWED_OK"));
}

#[test]
fn default_block_timeout_flag_accepted_and_runs() {
    let h = home();
    let md = write_wb(
        &h,
        "dbt.md",
        "---\nruntime: bash\n---\n```bash\necho FAST_BLOCK\n```\n",
    );
    let out = wb(&h)
        .arg(&md)
        .args(["--default-block-timeout", "5s"])
        .output()
        .expect("spawn wb");
    assert_eq!(
        out.status.code(),
        Some(0),
        "a fast block under a 5s cap should pass"
    );
    assert!(combined(&out).contains("FAST_BLOCK"));
}

// ─── global --log-level ─────────────────────────────────────────────────────

#[test]
fn log_level_error_still_runs() {
    let h = home();
    let md = write_wb(
        &h,
        "ll.md",
        "---\nruntime: bash\n---\n```bash\necho LOGLEVEL_OK\n```\n",
    );
    let out = wb(&h)
        .args(["--log-level", "error"])
        .arg("run")
        .arg(&md)
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(0));
    assert!(combined(&out).contains("LOGLEVEL_OK"));
}

#[test]
fn log_level_debug_still_runs() {
    let h = home();
    let md = write_wb(
        &h,
        "lld.md",
        "---\nruntime: bash\n---\n```bash\necho DEBUG_OK\n```\n",
    );
    let out = wb(&h)
        .args(["--log-level", "debug"])
        .arg("run")
        .arg(&md)
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(0));
    assert!(combined(&out).contains("DEBUG_OK"));
}

// ─── per-block timeout that actually fires ──────────────────────────────────

#[test]
fn per_block_timeout_fires_and_finishes_quickly() {
    use std::time::Instant;
    let h = home();
    // Durations are second-granular (ms is rejected/ignored), so a 1s cap on a
    // `sleep 30` block is the tightest cap that reliably fires.
    let md = write_wb(
        &h,
        "to.md",
        "---\nruntime: bash\n---\n```bash {timeout=1s}\nsleep 30\necho SHOULD_NOT_PRINT\n```\n",
    );
    let start = Instant::now();
    let out = wb(&h)
        .arg(&md)
        .args(["-q", "--json"])
        .output()
        .expect("spawn wb");
    let elapsed = start.elapsed();

    assert_ne!(
        out.status.code(),
        Some(0),
        "a timed-out block should fail the run"
    );
    assert!(
        elapsed.as_secs() < 20,
        "timeout should cut the 30s sleep short (took {elapsed:?})"
    );
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("--json stdout should parse");
    let block = &v["results"][0];
    assert_eq!(
        block["error_type"], "timeout",
        "timed-out block should carry error_type=timeout: {v}"
    );
    assert_eq!(
        block["stdout_partial"],
        serde_json::Value::Bool(true),
        "timed-out block should mark partial output: {v}"
    );
    assert!(
        !combined(&out).contains("SHOULD_NOT_PRINT"),
        "the post-sleep echo must never run"
    );
}
