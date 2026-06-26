//! Integration coverage for `src/lib.rs` run-lifecycle features, exercised by
//! spawning the real `wb` binary so llvm-cov instruments the production paths:
//! checkpoint resume, the `--events` JSONL sink, `{when=}`/`{skip_if=}`
//! conditionals (env + `$WB_OUT_*` gating), `include:`/`required:` composition,
//! and `wait` pause/resume/cancel/timeout-reap.
//!
//! Isolation: every test gets its own `tempfile::tempdir()` used as `$HOME`, so
//! all of `~/.wb` (checkpoints, pending descriptors, runs, config) is scoped to
//! the test and nothing touches the developer's real home. A checkpoint/resume
//! *pair* reuses ONE home across both spawns so the checkpoint persists.
//!
//! These tests must not depend on the network, Docker, Doppler, or Redis, and
//! keep `wait` timeouts short. `orchestration_contract.rs` already covers the
//! callback-payload shape of wait/browser pauses; here we cover the CLI gaps
//! (`--value`, `--signal -`, `--signal file`, `wb pending` listing + reaping,
//! `wb cancel`) plus the non-pause lifecycle features.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn wb_binary() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("wb")
}

/// Absolute path to the repo's `examples/` dir (for include/required fixtures).
fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples")
}

/// Run `wb <args>` with `home` as `$HOME` (isolating `~/.wb`). Optional stdin.
fn run_wb_in(home: &Path, args: &[&str], stdin: Option<&str>) -> std::process::Output {
    let mut cmd = Command::new(wb_binary());
    cmd.args(args)
        .env("HOME", home)
        // Make sure a stray WB_CHECKPOINT_DIR in the dev shell can't redirect
        // checkpoints away from our isolated $HOME.
        .env_remove("WB_CHECKPOINT_DIR")
        .env_remove("WB_CONFIG_PATH")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn wb");
    if let Some(input) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    child.wait_with_output().expect("wait wb")
}

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

fn stdout_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn diag(out: &std::process::Output) -> String {
    format!(
        "exit={:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

// ---------------------------------------------------------------------------
// Checkpoint resume
// ---------------------------------------------------------------------------

/// A failing middle block stops the run with `--bail`; after fixing it, a
/// re-run under the same `--checkpoint` resumes at the failed block. Already
/// completed block 1 is *replayed quietly* (WB_REPLAY=1) to rebuild session
/// state, so its live `echo` does NOT reappear in visible stdout — the operator
/// only sees the fixed block and everything after it.
#[test]
fn checkpoint_resumes_after_fixing_failed_block() {
    let home = tempfile::tempdir().unwrap();
    let wb = home.path().join("deploy.md");

    // Block 1 only prints BLOCK1_LIVE on a real (non-replay) run.
    let v1 = "```bash\nif [ -z \"$WB_REPLAY\" ]; then echo BLOCK1_LIVE; fi\n```\n\
              ```bash\nexit 1\n```\n\
              ```bash\necho BLOCK3_RAN\n```\n";
    write(&wb, v1);

    let r1 = run_wb_in(
        home.path(),
        &["run", wb.to_str().unwrap(), "--bail", "--checkpoint", "c1"],
        None,
    );
    assert_eq!(
        r1.status.code(),
        Some(1),
        "run 1 should fail at block 2:\n{}",
        diag(&r1)
    );
    let s1 = stdout_of(&r1);
    assert!(
        s1.contains("BLOCK1_LIVE"),
        "block 1 runs live first time:\n{s1}"
    );
    assert!(
        !s1.contains("BLOCK3_RAN"),
        "block 3 must not run after bail:\n{s1}"
    );

    // Fix block 2, keep the same block count so the run resumes (not fresh).
    let v2 = "```bash\nif [ -z \"$WB_REPLAY\" ]; then echo BLOCK1_LIVE; fi\n```\n\
              ```bash\necho BLOCK2_FIXED\n```\n\
              ```bash\necho BLOCK3_RAN\n```\n";
    write(&wb, v2);

    let r2 = run_wb_in(
        home.path(),
        &["run", wb.to_str().unwrap(), "--bail", "--checkpoint", "c1"],
        None,
    );
    assert!(r2.status.success(), "resume should succeed:\n{}", diag(&r2));
    let s2 = stdout_of(&r2);
    assert!(
        s2.contains("BLOCK2_FIXED"),
        "fixed block 2 should run on resume:\n{s2}"
    );
    assert!(
        s2.contains("BLOCK3_RAN"),
        "block 3 should run after resume:\n{s2}"
    );
    assert!(
        !s2.contains("BLOCK1_LIVE"),
        "completed block 1 should be replayed quietly, not re-run live:\n{s2}"
    );
}

/// A checkpoint that already completed starts fresh on the next run reusing the
/// same id (ids are reusable) — block 1 runs live again.
#[test]
fn completed_checkpoint_reruns_fresh() {
    let home = tempfile::tempdir().unwrap();
    let wb = home.path().join("done.md");
    write(&wb, "```bash\necho A_RAN\n```\n```bash\necho B_RAN\n```\n");

    let r1 = run_wb_in(
        home.path(),
        &["run", wb.to_str().unwrap(), "--checkpoint", "c1"],
        None,
    );
    assert!(
        r1.status.success(),
        "first run should complete:\n{}",
        diag(&r1)
    );

    let r2 = run_wb_in(
        home.path(),
        &["run", wb.to_str().unwrap(), "--checkpoint", "c1"],
        None,
    );
    assert!(r2.status.success(), "rerun should complete:\n{}", diag(&r2));
    let s2 = stdout_of(&r2);
    assert!(
        s2.contains("A_RAN"),
        "completed checkpoint reruns fresh:\n{s2}"
    );
    assert!(
        s2.contains("B_RAN"),
        "completed checkpoint reruns fresh:\n{s2}"
    );
}

/// When the block count shrinks below the saved resume index, the checkpoint
/// can't recover the position and starts fresh — block 1 re-runs live.
#[test]
fn changed_block_count_starts_fresh() {
    let home = tempfile::tempdir().unwrap();
    let wb = home.path().join("shrink.md");

    // 4 blocks; the last one fails under --bail, so the saved next_block is 3.
    let four = "```bash\nif [ -z \"$WB_REPLAY\" ]; then echo X_LIVE; fi\n```\n\
                ```bash\necho B2\n```\n\
                ```bash\necho B3\n```\n\
                ```bash\nexit 1\n```\n";
    write(&wb, four);
    let r1 = run_wb_in(
        home.path(),
        &[
            "run",
            wb.to_str().unwrap(),
            "--bail",
            "--checkpoint",
            "shrink",
        ],
        None,
    );
    assert_eq!(
        r1.status.code(),
        Some(1),
        "run 1 fails at block 4:\n{}",
        diag(&r1)
    );

    // Shrink to 2 blocks: saved next_block (3) is now out of range → fresh.
    let two = "```bash\nif [ -z \"$WB_REPLAY\" ]; then echo X_LIVE; fi\n```\n\
               ```bash\necho Y\n```\n";
    write(&wb, two);
    let r2 = run_wb_in(
        home.path(),
        &[
            "run",
            wb.to_str().unwrap(),
            "--bail",
            "--checkpoint",
            "shrink",
        ],
        None,
    );
    assert!(
        r2.status.success(),
        "fresh run should complete:\n{}",
        diag(&r2)
    );
    let s2 = stdout_of(&r2);
    assert!(
        s2.contains("X_LIVE"),
        "changed block count should start fresh, re-running block 1 live:\n{s2}"
    );
}

// ---------------------------------------------------------------------------
// --events JSONL sink
// ---------------------------------------------------------------------------

/// `--events run.jsonl` appends one JSON object per line; each parses, carries
/// an `event` field, and the stream includes `step.complete` per block plus a
/// terminal `run.complete`.
#[test]
fn events_jsonl_has_one_json_object_per_line() {
    let home = tempfile::tempdir().unwrap();
    let wb = home.path().join("ev.md");
    write(&wb, "```bash\necho one\n```\n```bash\necho two\n```\n");
    let events = home.path().join("run.jsonl");

    let out = run_wb_in(
        home.path(),
        &[
            "run",
            wb.to_str().unwrap(),
            "--events",
            events.to_str().unwrap(),
        ],
        None,
    );
    assert!(out.status.success(), "run should succeed:\n{}", diag(&out));
    assert!(events.exists(), "events file should be written");

    let text = std::fs::read_to_string(&events).unwrap();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(
        lines.len() >= 3,
        "expected >=3 event lines, got {}:\n{text}",
        lines.len()
    );

    let mut kinds = Vec::new();
    for line in &lines {
        let v: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("line is not JSON ({e}): {line}"));
        kinds.push(v["event"].as_str().unwrap_or("").to_string());
    }
    let n_step = kinds.iter().filter(|k| *k == "step.complete").count();
    assert_eq!(
        n_step, 2,
        "two blocks → two step.complete events:\n{kinds:?}"
    );
    assert_eq!(
        kinds.last().map(String::as_str),
        Some("run.complete"),
        "stream should end with run.complete:\n{kinds:?}"
    );
}

// ---------------------------------------------------------------------------
// Conditionals: {when=} / {skip_if=} on env, and $WB_OUT_ gating
// ---------------------------------------------------------------------------

const COND_WB: &str = "```bash {when=$DEPLOY=prod}\necho WHEN_RAN\n```\n\
                       ```bash {skip_if=$DRY}\necho LIVE_RAN\n```\n";

#[test]
fn conditional_when_runs_and_skip_if_runs_when_env_permits() {
    let home = tempfile::tempdir().unwrap();
    let wb = home.path().join("cond.md");
    write(&wb, COND_WB);

    // DEPLOY=prod → when fires; DRY unset → skip_if block runs.
    let out = run_wb_in(
        home.path(),
        &["run", wb.to_str().unwrap(), "-e", "DEPLOY=prod"],
        None,
    );
    assert!(out.status.success(), "{}", diag(&out));
    let s = stdout_of(&out);
    assert!(s.contains("WHEN_RAN"), "when=prod should fire:\n{s}");
    assert!(s.contains("LIVE_RAN"), "skip_if unset should run:\n{s}");
}

#[test]
fn conditional_when_skips_and_skip_if_skips() {
    let home = tempfile::tempdir().unwrap();
    let wb = home.path().join("cond.md");
    write(&wb, COND_WB);

    // No DEPLOY → when skipped; DRY=1 → skip_if block skipped.
    let out = run_wb_in(
        home.path(),
        &["run", wb.to_str().unwrap(), "-e", "DRY=1"],
        None,
    );
    assert!(out.status.success(), "{}", diag(&out));
    let s = stdout_of(&out);
    assert!(
        !s.contains("WHEN_RAN"),
        "when= should be skipped without DEPLOY=prod:\n{s}"
    );
    assert!(
        !s.contains("LIVE_RAN"),
        "skip_if=$DRY should skip when DRY=1:\n{s}"
    );
}

#[test]
fn conditional_gates_on_prior_step_output_truthy() {
    let home = tempfile::tempdir().unwrap();
    let wb = home.path().join("gate.md");
    write(
        &wb,
        "```bash\necho \"output: needs_login=1\"\n```\n\
         ```bash {when=$WB_OUT_needs_login}\necho GUARD_RAN\n```\n\
         ```bash {skip_if=$WB_OUT_needs_login}\necho WARM_RAN\n```\n",
    );
    let out = run_wb_in(home.path(), &["run", wb.to_str().unwrap()], None);
    assert!(out.status.success(), "{}", diag(&out));
    let s = stdout_of(&out);
    assert!(
        s.contains("GUARD_RAN"),
        "when=$WB_OUT_needs_login should fire on 1:\n{s}"
    );
    assert!(
        !s.contains("WARM_RAN"),
        "skip_if should suppress warm block on 1:\n{s}"
    );
}

#[test]
fn conditional_gates_on_prior_step_output_falsy() {
    let home = tempfile::tempdir().unwrap();
    let wb = home.path().join("gate.md");
    write(
        &wb,
        "```bash\necho \"output: needs_login=0\"\n```\n\
         ```bash {when=$WB_OUT_needs_login}\necho GUARD_RAN\n```\n\
         ```bash {skip_if=$WB_OUT_needs_login}\necho WARM_RAN\n```\n",
    );
    let out = run_wb_in(home.path(), &["run", wb.to_str().unwrap()], None);
    assert!(out.status.success(), "{}", diag(&out));
    let s = stdout_of(&out);
    assert!(!s.contains("GUARD_RAN"), "when should skip on 0:\n{s}");
    assert!(
        s.contains("WARM_RAN"),
        "skip_if=0 should run warm block:\n{s}"
    );
}

// ---------------------------------------------------------------------------
// include: / required:
// ---------------------------------------------------------------------------

#[test]
fn include_demo_splices_prerequisite_blocks() {
    let home = tempfile::tempdir().unwrap();
    let demo = examples_dir().join("include-demo.md");
    let out = run_wb_in(home.path(), &["run", demo.to_str().unwrap()], None);
    assert!(
        out.status.success(),
        "include-demo should run:\n{}",
        diag(&out)
    );
    let s = stdout_of(&out);
    // The included login block runs, then the parent's deploy block reads its
    // artifact back in the same run.
    assert!(
        s.contains("logging in as"),
        "included login block should run:\n{s}"
    );
    assert!(
        s.contains("deploy step"),
        "parent block should run after include:\n{s}"
    );
}

#[test]
fn required_frontmatter_runs_prerequisites_first() {
    let home = tempfile::tempdir().unwrap();
    let demo = examples_dir().join("required-demo.md");
    let out = run_wb_in(home.path(), &["run", demo.to_str().unwrap()], None);
    assert!(
        out.status.success(),
        "required-demo should run:\n{}",
        diag(&out)
    );
    let s = stdout_of(&out);
    assert!(
        s.contains("logging in as"),
        "required prerequisite should run first:\n{s}"
    );
    assert!(
        s.contains("session from required prerequisite"),
        "parent block should run after required prereq:\n{s}"
    );
}

#[test]
fn missing_include_target_exits_workbook_invalid() {
    let home = tempfile::tempdir().unwrap();
    let wb = home.path().join("missing.md");
    write(&wb, "```include\npath: ./does-not-exist.md\n```\n");
    let out = run_wb_in(home.path(), &["run", wb.to_str().unwrap()], None);
    assert_eq!(
        out.status.code(),
        Some(3),
        "missing include target → EXIT_WORKBOOK_INVALID:\n{}",
        diag(&out)
    );
}

#[test]
fn circular_include_exits_workbook_invalid() {
    let home = tempfile::tempdir().unwrap();
    let a = home.path().join("a.md");
    let b = home.path().join("b.md");
    write(&a, "```include\npath: ./b.md\n```\n");
    write(&b, "```include\npath: ./a.md\n```\n");
    let out = run_wb_in(home.path(), &["run", a.to_str().unwrap()], None);
    assert_eq!(
        out.status.code(),
        Some(3),
        "circular include → EXIT_WORKBOOK_INVALID:\n{}",
        diag(&out)
    );
}

// ---------------------------------------------------------------------------
// wait / pause / resume / cancel / timeout reap
// ---------------------------------------------------------------------------

/// Minimal manual-wait workbook bound to `code`, with a long timeout so it
/// stays paused for the resume tests.
fn write_wait_workbook(path: &Path) {
    write(
        path,
        "```bash\necho starting\n```\n\
         ```wait\nkind: manual\nbind: code\ntimeout: 1h\non_timeout: abort\n```\n\
         ```bash\necho \"got=$code\"\n```\n",
    );
}

#[test]
fn wait_pauses_with_exit_42_and_writes_pending_descriptor() {
    let home = tempfile::tempdir().unwrap();
    let wb = home.path().join("wait.md");
    write_wait_workbook(&wb);

    let out = run_wb_in(
        home.path(),
        &["run", wb.to_str().unwrap(), "--checkpoint", "w42"],
        None,
    );
    assert_eq!(
        out.status.code(),
        Some(42),
        "wait should pause with code 42:\n{}",
        diag(&out)
    );

    let pending = home.path().join(".wb/checkpoints/w42.pending.json");
    assert!(pending.exists(), "pending descriptor should be written");
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&pending).unwrap()).unwrap();
    assert_eq!(v["on_timeout"], "abort");
}

#[test]
fn wb_pending_lists_a_paused_run() {
    let home = tempfile::tempdir().unwrap();
    let wb = home.path().join("wait.md");
    write_wait_workbook(&wb);
    let _ = run_wb_in(
        home.path(),
        &["run", wb.to_str().unwrap(), "--checkpoint", "plist"],
        None,
    );

    let out = run_wb_in(home.path(), &["pending", "--no-reap"], None);
    assert!(
        out.status.success(),
        "pending should succeed:\n{}",
        diag(&out)
    );
    let s = stdout_of(&out);
    assert!(
        s.contains("plist"),
        "pending list should include the checkpoint id:\n{s}"
    );
}

#[test]
fn resume_with_value_completes_paused_run() {
    let home = tempfile::tempdir().unwrap();
    let wb = home.path().join("wait.md");
    write_wait_workbook(&wb);
    let paused = run_wb_in(
        home.path(),
        &["run", wb.to_str().unwrap(), "--checkpoint", "rv"],
        None,
    );
    assert_eq!(paused.status.code(), Some(42), "{}", diag(&paused));

    let resumed = run_wb_in(home.path(), &["resume", "rv", "--value", "HELLO"], None);
    assert!(
        resumed.status.success(),
        "resume --value should succeed:\n{}",
        diag(&resumed)
    );
    assert!(
        stdout_of(&resumed).contains("got=HELLO"),
        "bound value should reach the next block:\n{}",
        stdout_of(&resumed)
    );
    assert!(
        !home.path().join(".wb/checkpoints/rv.pending.json").exists(),
        "pending descriptor should be cleared after resume"
    );
}

#[test]
fn resume_with_signal_stdin_completes_paused_run() {
    let home = tempfile::tempdir().unwrap();
    let wb = home.path().join("wait.md");
    write_wait_workbook(&wb);
    let paused = run_wb_in(
        home.path(),
        &["run", wb.to_str().unwrap(), "--checkpoint", "rs"],
        None,
    );
    assert_eq!(paused.status.code(), Some(42), "{}", diag(&paused));

    let resumed = run_wb_in(
        home.path(),
        &["resume", "rs", "--signal", "-"],
        Some("{\"code\":\"VIA_STDIN\"}"),
    );
    assert!(
        resumed.status.success(),
        "resume --signal - should succeed:\n{}",
        diag(&resumed)
    );
    assert!(
        stdout_of(&resumed).contains("got=VIA_STDIN"),
        "JSON payload over stdin should bind the value:\n{}",
        stdout_of(&resumed)
    );
}

#[test]
fn resume_with_signal_file_completes_paused_run() {
    let home = tempfile::tempdir().unwrap();
    let wb = home.path().join("wait.md");
    write_wait_workbook(&wb);
    let paused = run_wb_in(
        home.path(),
        &["run", wb.to_str().unwrap(), "--checkpoint", "rf"],
        None,
    );
    assert_eq!(paused.status.code(), Some(42), "{}", diag(&paused));

    let sig = home.path().join("sig.json");
    write(&sig, "{\"code\":\"FROM_FILE\"}");
    let resumed = run_wb_in(
        home.path(),
        &["resume", "rf", "--signal", sig.to_str().unwrap()],
        None,
    );
    assert!(
        resumed.status.success(),
        "resume --signal file should succeed:\n{}",
        diag(&resumed)
    );
    assert!(
        stdout_of(&resumed).contains("got=FROM_FILE"),
        "file payload should bind the value:\n{}",
        stdout_of(&resumed)
    );
}

#[test]
fn cancel_drops_a_paused_run() {
    let home = tempfile::tempdir().unwrap();
    let wb = home.path().join("wait.md");
    write_wait_workbook(&wb);
    let paused = run_wb_in(
        home.path(),
        &["run", wb.to_str().unwrap(), "--checkpoint", "cx"],
        None,
    );
    assert_eq!(paused.status.code(), Some(42), "{}", diag(&paused));

    let cancelled = run_wb_in(home.path(), &["cancel", "cx"], None);
    assert!(
        cancelled.status.success(),
        "cancel should succeed:\n{}",
        diag(&cancelled)
    );
    assert!(
        !home.path().join(".wb/checkpoints/cx.pending.json").exists(),
        "cancel should drop the pending descriptor"
    );

    // No longer listed as pending.
    let after = run_wb_in(home.path(), &["pending", "--no-reap"], None);
    assert!(
        !stdout_of(&after).contains("cx"),
        "cancelled run should not be pending:\n{}",
        stdout_of(&after)
    );
}

#[test]
fn expired_wait_is_reaped_by_wb_pending() {
    let home = tempfile::tempdir().unwrap();
    let wb = home.path().join("short.md");
    // 1s timeout, on_timeout: abort → reaped once it expires.
    write(
        &wb,
        "```bash\necho go\n```\n\
         ```wait\nkind: manual\nbind: code\ntimeout: 1s\non_timeout: abort\n```\n\
         ```bash\necho \"got=$code\"\n```\n",
    );
    let paused = run_wb_in(
        home.path(),
        &["run", wb.to_str().unwrap(), "--checkpoint", "reapme"],
        None,
    );
    assert_eq!(paused.status.code(), Some(42), "{}", diag(&paused));

    // Wait for the descriptor's timeout_at to pass.
    std::thread::sleep(std::time::Duration::from_millis(1500));

    let reaped = run_wb_in(home.path(), &["pending"], None);
    assert!(
        reaped.status.success(),
        "pending+reap should succeed:\n{}",
        diag(&reaped)
    );

    assert!(
        !home
            .path()
            .join(".wb/checkpoints/reapme.pending.json")
            .exists(),
        "expired abort-mode pending descriptor should be reaped"
    );
    let ckpt = home.path().join(".wb/checkpoints/reapme.json");
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&ckpt).unwrap()).unwrap();
    assert_eq!(
        v["status"], "failed",
        "reaped checkpoint should be marked failed"
    );
}

// ---------------------------------------------------------------------------
// Callback URL validation (fails fast, before running)
// ---------------------------------------------------------------------------

#[test]
fn malformed_callback_url_fails_fast() {
    let home = tempfile::tempdir().unwrap();
    let wb = home.path().join("cb.md");
    // A marker block: if it ran, we'd see it on stdout. It must NOT run.
    write(&wb, "```bash\necho SHOULD_NOT_RUN\n```\n");

    let out = run_wb_in(
        home.path(),
        &["run", wb.to_str().unwrap(), "--callback", "ftp://nope"],
        None,
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "bad callback scheme → EXIT_USAGE:\n{}",
        diag(&out)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unsupported callback URL"),
        "should explain the bad scheme:\n{stderr}"
    );
    assert!(
        !stdout_of(&out).contains("SHOULD_NOT_RUN"),
        "validation must fail before any block runs:\n{}",
        stdout_of(&out)
    );
}
