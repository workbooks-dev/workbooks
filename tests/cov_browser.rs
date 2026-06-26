//! Integration coverage for wb's browser-sidecar path, driven by a FAKE
//! sidecar (a `#!/bin/sh` script pointed at via `WB_BROWSER_RUNTIME`). No real
//! browser / `wb-browser-runtime` is needed — the script speaks the
//! `wb-sidecar/1` line-framed JSON protocol over stdin/stdout and always
//! terminates with a terminal frame, so wb never blocks.
//!
//! These tests spawn the real `wb` binary so cargo-llvm-cov instruments the
//! spawn/handshake/run_slice/suspend/Drop paths in `src/sidecar.rs` plus the
//! browser-slice execution in `src/executor.rs` and `src/lib.rs`.
//!
//! Reverse-engineered frame protocol (see `src/sidecar.rs`):
//!   wb → sidecar (newline-delimited JSON):
//!     {"type":"hello","wb_version":..,"protocol":"wb-sidecar/1"}   (handshake)
//!     {"type":"slice","session":..,"verbs":[..],"restore":{..}?}   (per slice)
//!     {"type":"suspend"}    (pause: keep browser alive, sidecar should exit)
//!     {"type":"shutdown"}   (Drop: clean teardown, sidecar should exit)
//!   sidecar → wb:
//!     {"type":"ready"}                          (handshake reply)
//!     {"type":"verb.complete","verb":..,"summary":..}
//!     {"type":"verb.failed","verb":..,"error":..}
//!     {"type":"slice.recovered",..}             (→ step.recovered callback)
//!     {"type":"slice.artifact_saved",..}        (no-op in wb; disk drives it)
//!     {"type":"slice.download_skipped",..}      (→ step.download_skipped cb)
//!     {"type":"slice.<other>",..}               (→ step.<other> passthrough)
//!     {"type":"slice.paused","reason":..,"sidecar_state":..,"actions":..}
//!     {"type":"slice.complete"}                 (terminal: success)
//!     {"type":"slice.failed","error":..}        (terminal: failure)

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn wb_binary() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("wb")
}

/// One flexible fake sidecar reused by every test. Behavior is selected through
/// `WB_FAKE_*` env vars set on the `wb` command (inherited by the sidecar
/// subprocess). Always emits a terminal frame and exits on suspend/shutdown.
const FAKE_SIDECAR: &str = r#"#!/bin/sh
emit() { printf '%s\n' "$1"; }

while IFS= read -r line; do
  case "$line" in
    *'"type":"hello"'*)
      if [ -n "$WB_FAKE_EXIT_ON_HELLO" ]; then
        exit 0
      elif [ -n "$WB_FAKE_BAD_READY" ]; then
        emit '{"type":"not-ready"}'
        exit 0
      else
        emit '{"type":"ready"}'
      fi
      ;;
    *'"type":"slice"'*)
      if [ -n "$WB_FAKE_ECHO_ENV" ]; then
        emit "{\"type\":\"verb.complete\",\"verb\":\"echo-env\",\"summary\":\"exts=${WB_BROWSER_DOWNLOAD_EXTENSIONS}\"}"
      fi
      emit '{"type":"verb.complete","verb":"goto","summary":"navigated"}'
      if [ -n "$WB_FAKE_RECOVERED" ]; then
        emit '{"type":"slice.recovered","verb":"click","recovered_selector":"button.ok"}'
      fi
      if [ -n "$WB_FAKE_ARTIFACT" ] && [ -n "$WB_ARTIFACTS_DIR" ]; then
        mkdir -p "$WB_ARTIFACTS_DIR"
        printf '%s' "${WB_FAKE_ARTIFACT_CONTENT:-fake-bytes}" > "$WB_ARTIFACTS_DIR/$WB_FAKE_ARTIFACT"
        if [ -n "$WB_FAKE_ARTIFACT_META" ]; then
          printf '{"label":"Fake report","description":"from fake sidecar"}' > "$WB_ARTIFACTS_DIR/$WB_FAKE_ARTIFACT.meta.json"
        fi
      fi
      if [ -n "$WB_FAKE_ARTIFACT_SAVED" ]; then
        emit '{"type":"slice.artifact_saved","filename":"download.bin","source_url":"https://example.com/download.bin"}'
      fi
      if [ -n "$WB_FAKE_DOWNLOAD_SKIPPED" ]; then
        emit '{"type":"slice.download_skipped","filename":"big.iso","reason":"extension_filtered"}'
      fi
      if [ -n "$WB_FAKE_VERB_FAIL" ]; then
        emit '{"type":"verb.failed","verb":"click","error":"selector not found"}'
        emit '{"type":"slice.failed","error":"verb click failed"}'
      elif [ -n "$WB_FAKE_FAIL" ]; then
        emit '{"type":"slice.failed","error":"navigation crashed"}'
      elif [ -n "$WB_FAKE_SENTINEL" ]; then
        if [ -f "$WB_FAKE_SENTINEL" ]; then
          emit '{"type":"verb.complete","verb":"continue","summary":"resumed"}'
          emit '{"type":"slice.complete"}'
        else
          : > "$WB_FAKE_SENTINEL"
          emit '{"type":"slice.paused","reason":"pause_for_human","resume_url":"https://live.example/s","verb_index":0,"message":"Approve this task","context_url":"https://example.com/task","resume_on":"operator_click","timeout":"30m","actions":[{"kind":"goto_step","target":"after","label":"Skip ahead"}],"sidecar_state":{"session":"abc"}}'
        fi
      else
        emit '{"type":"slice.complete"}'
      fi
      ;;
    *'"type":"suspend"'*|*'"type":"shutdown"'*)
      exit 0
      ;;
  esac
done
"#;

fn write_fake_sidecar(dir: &Path) -> PathBuf {
    let path = dir.join("fake-sidecar.sh");
    std::fs::write(&path, FAKE_SIDECAR).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    path
}

/// Parse a `--events` JSONL file into one Value per line.
fn read_events(path: &Path) -> Vec<serde_json::Value> {
    let raw = std::fs::read_to_string(path).unwrap_or_default();
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn event_names(events: &[serde_json::Value]) -> Vec<String> {
    events
        .iter()
        .map(|e| e["event"].as_str().unwrap_or("").to_string())
        .collect()
}

/// A `wb run <workbook> --events <file>` command pre-seeded with the standard
/// isolation env (HOME, fake sidecar, deterministic artifacts dir + run id, a
/// short sidecar shutdown window so Drop/suspend never linger).
fn base_run(
    home: &Path,
    sidecar: &Path,
    artifacts: &Path,
    workbook: &Path,
    events: &Path,
) -> Command {
    let mut cmd = Command::new(wb_binary());
    cmd.args([
        "run",
        workbook.to_str().unwrap(),
        "--events",
        events.to_str().unwrap(),
    ])
    .env("HOME", home)
    .env("WB_BROWSER_RUNTIME", sidecar)
    .env("WB_ARTIFACTS_DIR", artifacts)
    .env("WB_RECORDING_RUN_ID", "cov-browser-run")
    .env("WB_SIDECAR_SHUTDOWN_TIMEOUT_SECS", "5")
    .env("WB_LOG_LEVEL", "warn");
    cmd
}

fn write(path: &Path, contents: &str) {
    std::fs::write(path, contents).unwrap();
}

const SIMPLE_BROWSER_WB: &str = r#"---
title: Browser cov
---
```bash
echo "host pre-check"
```

## Browser slice

```browser {#nav}
session: cov
verbs:
  - goto: https://example.com
  - click: "button.ok"
```

## After

```bash
echo "host post-check"
```
"#;

// --- happy path: spawn → handshake → run_slice → complete → Drop ----------

#[test]
fn browser_slice_runs_to_completion() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let sidecar = write_fake_sidecar(dir.path());
    let artifacts = dir.path().join("artifacts");
    let wb = dir.path().join("simple.md");
    let events = dir.path().join("events.jsonl");
    write(&wb, SIMPLE_BROWSER_WB);

    let out = base_run(&home, &sidecar, &artifacts, &wb, &events)
        .output()
        .expect("spawn wb");
    assert!(
        out.status.success(),
        "expected success\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    // The sidecar stayed alive between host blocks (post-check ran after the
    // browser slice).
    assert!(stdout.contains("host post-check"), "stdout:\n{stdout}");

    let evs = read_events(&events);
    let names = event_names(&evs);
    assert!(
        names.iter().filter(|n| *n == "step.complete").count() >= 3,
        "expected step.complete for both bash blocks + browser slice: {names:?}"
    );
    let run = evs.iter().find(|e| e["event"] == "run.complete").unwrap();
    assert_eq!(run["data"]["status"], "pass");
    assert_eq!(run["data"]["blocks"]["total"], 3);
}

// --- extract/save: sidecar writes an artifact into $WB_ARTIFACTS_DIR -------

#[test]
fn browser_slice_saves_artifact_to_dir() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let sidecar = write_fake_sidecar(dir.path());
    let artifacts = dir.path().join("artifacts");
    let wb = dir.path().join("artifact.md");
    let events = dir.path().join("events.jsonl");
    write(
        &wb,
        r#"---
title: Browser artifact
---
```browser {#extract}
session: cov
verbs:
  - extract: "table.results"
  - save: report.csv
```
"#,
    );

    let out = base_run(&home, &sidecar, &artifacts, &wb, &events)
        .env("WB_FAKE_ARTIFACT", "report.csv")
        .env("WB_FAKE_ARTIFACT_CONTENT", "id,name\n1,Ada\n")
        .env("WB_FAKE_ARTIFACT_META", "1")
        .output()
        .expect("spawn wb");
    assert!(
        out.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The file the sidecar wrote into $WB_ARTIFACTS_DIR is present.
    let saved = artifacts.join("report.csv");
    assert!(saved.is_file(), "artifact file should exist at {saved:?}");
    assert_eq!(std::fs::read_to_string(&saved).unwrap(), "id,name\n1,Ada\n");

    // wb's post-slice sync picked it up and fired a step.artifact_saved event,
    // carrying the sidecar's .meta.json label/description.
    let evs = read_events(&events);
    let art = evs
        .iter()
        .find(|e| e["event"] == "step.artifact_saved")
        .expect("expected a step.artifact_saved event");
    assert_eq!(art["data"]["artifact"]["filename"], "report.csv");
    assert_eq!(art["data"]["artifact"]["label"], "Fake report");
}

// --- slice.artifact_saved (auto-download) + slice.download_skipped frames --

#[test]
fn browser_slice_emits_artifact_saved_and_download_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let sidecar = write_fake_sidecar(dir.path());
    let artifacts = dir.path().join("artifacts");
    let wb = dir.path().join("downloads.md");
    let events = dir.path().join("events.jsonl");
    write(
        &wb,
        r#"---
title: Browser downloads
---
```browser {#dl}
session: cov
verbs:
  - click: "a.attachment"
```
"#,
    );

    let out = base_run(&home, &sidecar, &artifacts, &wb, &events)
        // auto-download: emit the frame AND drop the file on disk (wb's feed is
        // driven by the on-disk file → step.artifact_saved, not by the frame).
        .env("WB_FAKE_ARTIFACT_SAVED", "1")
        .env("WB_FAKE_ARTIFACT", "download.bin")
        .env("WB_FAKE_DOWNLOAD_SKIPPED", "1")
        .output()
        .expect("spawn wb");
    assert!(
        out.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let evs = read_events(&events);
    let names = event_names(&evs);
    // download_skipped flows through the generic slice.* → step.* passthrough.
    assert!(
        names.iter().any(|n| n == "step.download_skipped"),
        "expected step.download_skipped passthrough: {names:?}"
    );
    let skipped = evs
        .iter()
        .find(|e| e["event"] == "step.download_skipped")
        .unwrap();
    assert_eq!(skipped["data"]["filename"], "big.iso");
    // The auto-captured download surfaced as a persisted artifact.
    assert!(
        names.iter().any(|n| n == "step.artifact_saved"),
        "expected step.artifact_saved for the captured download: {names:?}"
    );
    assert!(artifacts.join("download.bin").is_file());
}

// --- slice.recovered → step.recovered callback ----------------------------

#[test]
fn browser_slice_recovered_fires_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let sidecar = write_fake_sidecar(dir.path());
    let artifacts = dir.path().join("artifacts");
    let wb = dir.path().join("recover.md");
    let events = dir.path().join("events.jsonl");
    write(
        &wb,
        r#"---
title: Browser recover
---
```browser {#act}
session: cov
verbs:
  - act: "click the approve button"
```
"#,
    );

    let out = base_run(&home, &sidecar, &artifacts, &wb, &events)
        .env("WB_FAKE_RECOVERED", "1")
        .output()
        .expect("spawn wb");
    assert!(out.status.success());

    let names = event_names(&read_events(&events));
    assert!(
        names.iter().any(|n| n == "step.recovered"),
        "expected step.recovered: {names:?}"
    );
}

// --- env propagation: wb forwards env into the sidecar subprocess ----------

#[test]
fn browser_env_propagates_to_sidecar() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let sidecar = write_fake_sidecar(dir.path());
    let artifacts = dir.path().join("artifacts");
    let wb = dir.path().join("env.md");
    let events = dir.path().join("events.jsonl");
    write(&wb, SIMPLE_BROWSER_WB);

    let out = base_run(&home, &sidecar, &artifacts, &wb, &events)
        .env("WB_FAKE_ECHO_ENV", "1")
        .env("WB_BROWSER_DOWNLOAD_EXTENSIONS", "pdf,xlsx,csv")
        .output()
        .expect("spawn wb");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // The sidecar echoed the env var wb forwarded to it, proving propagation.
    assert!(
        stdout.contains("exts=pdf,xlsx,csv"),
        "expected sidecar to see WB_BROWSER_DOWNLOAD_EXTENSIONS:\n{stdout}"
    );
}

// --- failure: terminal slice.failed → failed browser block ----------------

#[test]
fn browser_slice_failure_surfaces() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let sidecar = write_fake_sidecar(dir.path());
    let artifacts = dir.path().join("artifacts");
    let wb = dir.path().join("fail.md");
    let events = dir.path().join("events.jsonl");
    write(
        &wb,
        r#"---
title: Browser fail
---
```browser {#boom}
session: cov
verbs:
  - goto: https://example.com
```
"#,
    );

    let out = base_run(&home, &sidecar, &artifacts, &wb, &events)
        .env("WB_FAKE_FAIL", "1")
        .output()
        .expect("spawn wb");
    assert_eq!(
        out.status.code(),
        Some(1),
        "a failed slice should exit 1\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run = read_events(&events)
        .into_iter()
        .find(|e| e["event"] == "run.complete")
        .unwrap();
    assert_eq!(run["data"]["status"], "fail");
    assert_eq!(run["data"]["blocks"]["failed"], 1);
}

// --- verb.failed then slice.failed (exercises the verb.failed branch) -----

#[test]
fn browser_verb_failed_surfaces() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let sidecar = write_fake_sidecar(dir.path());
    let artifacts = dir.path().join("artifacts");
    let wb = dir.path().join("verbfail.md");
    let events = dir.path().join("events.jsonl");
    write(
        &wb,
        r#"---
title: Browser verb fail
---
```browser {#click}
session: cov
verbs:
  - click: "button.missing"
```
"#,
    );

    let out = base_run(&home, &sidecar, &artifacts, &wb, &events)
        .env("WB_FAKE_VERB_FAIL", "1")
        .output()
        .expect("spawn wb");
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("selector not found") || stderr.contains("✗"),
        "verb failure should surface on stderr:\n{stderr}"
    );
}

// --- pause/resume: pause_for_human → exit 42, then resume forward ----------

const PAUSE_BROWSER_WB: &str = r#"---
title: Browser pause
---
## Pause

```browser {#pause}
session: cov
verbs:
  - pause_for_human:
      message: Approve this task
      resume_on: operator_click
```

## After

```bash {#after}
echo "after-resume"
```
"#;

#[allow(clippy::too_many_arguments)]
fn pause_run(
    home: &Path,
    sidecar: &Path,
    artifacts: &Path,
    checkpoints: &Path,
    sentinel: &Path,
    workbook: &Path,
    events: &Path,
    checkpoint_id: &str,
) -> Command {
    let mut cmd = Command::new(wb_binary());
    cmd.args([
        "run",
        workbook.to_str().unwrap(),
        "--checkpoint",
        checkpoint_id,
        "--events",
        events.to_str().unwrap(),
    ])
    .env("HOME", home)
    .env("WB_BROWSER_RUNTIME", sidecar)
    .env("WB_ARTIFACTS_DIR", artifacts)
    .env("WB_CHECKPOINT_DIR", checkpoints)
    .env("WB_FAKE_SENTINEL", sentinel)
    .env("WB_RECORDING_RUN_ID", "cov-browser-pause")
    .env("WB_SIDECAR_SHUTDOWN_TIMEOUT_SECS", "5")
    .env("WB_LOG_LEVEL", "warn");
    cmd
}

fn resume_cmd(
    home: &Path,
    sidecar: &Path,
    artifacts: &Path,
    checkpoints: &Path,
    sentinel: &Path,
    checkpoint_id: &str,
) -> Command {
    let mut cmd = Command::new(wb_binary());
    cmd.arg("resume")
        .arg(checkpoint_id)
        .env("HOME", home)
        .env("WB_BROWSER_RUNTIME", sidecar)
        .env("WB_ARTIFACTS_DIR", artifacts)
        .env("WB_CHECKPOINT_DIR", checkpoints)
        .env("WB_FAKE_SENTINEL", sentinel)
        .env("WB_RECORDING_RUN_ID", "cov-browser-pause")
        .env("WB_SIDECAR_SHUTDOWN_TIMEOUT_SECS", "5")
        .env("WB_LOG_LEVEL", "warn");
    cmd
}

struct PauseFixture {
    _dir: tempfile::TempDir,
    home: PathBuf,
    sidecar: PathBuf,
    artifacts: PathBuf,
    checkpoints: PathBuf,
    sentinel: PathBuf,
    workbook: PathBuf,
    events: PathBuf,
}

/// Cold-run a pause workbook to the `pause_for_human` stop: assert exit 42 and a
/// pending descriptor on disk. Returns the fixture so the test can resume.
fn pause_to_42(checkpoint_id: &str) -> PauseFixture {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let sidecar = write_fake_sidecar(dir.path());
    let artifacts = dir.path().join("artifacts");
    let checkpoints = dir.path().join("checkpoints");
    let sentinel = dir.path().join("sentinel");
    let workbook = dir.path().join("pause.md");
    let events = dir.path().join("events.jsonl");
    write(&workbook, PAUSE_BROWSER_WB);

    let out = pause_run(
        &home,
        &sidecar,
        &artifacts,
        &checkpoints,
        &sentinel,
        &workbook,
        &events,
        checkpoint_id,
    )
    .output()
    .expect("spawn wb run");
    assert_eq!(
        out.status.code(),
        Some(42),
        "browser pause should exit 42\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let names = event_names(&read_events(&events));
    assert!(
        names.iter().any(|n| n == "step.paused"),
        "expected step.paused: {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "workbook.paused"),
        "expected workbook.paused: {names:?}"
    );
    let pending = checkpoints.join(format!("{checkpoint_id}.pending.json"));
    assert!(pending.is_file(), "pending descriptor should exist");

    PauseFixture {
        _dir: dir,
        home,
        sidecar,
        artifacts,
        checkpoints,
        sentinel,
        workbook,
        events,
    }
}

#[test]
fn browser_pause_then_resume_forward() {
    let id = "cov-pause-forward";
    let fx = pause_to_42(id);
    let _ = &fx.workbook;
    let _ = &fx.events;

    let out = resume_cmd(
        &fx.home,
        &fx.sidecar,
        &fx.artifacts,
        &fx.checkpoints,
        &fx.sentinel,
        id,
    )
    .args(["--value", "ok"])
    .output()
    .expect("spawn wb resume");
    assert!(
        out.status.success(),
        "forward resume should complete\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("after-resume"),
        "the post-pause bash block should run:\n{stdout}"
    );
    // Pending descriptor cleared once the resumed slice completed.
    let pending = fx.checkpoints.join(format!("{id}.pending.json"));
    assert!(!pending.exists(), "pending should be removed after resume");
}

#[test]
fn browser_pause_then_resume_rerun_step() {
    let id = "cov-pause-rerun";
    let fx = pause_to_42(id);

    let out = resume_cmd(
        &fx.home,
        &fx.sidecar,
        &fx.artifacts,
        &fx.checkpoints,
        &fx.sentinel,
        id,
    )
    .arg("--rerun-step")
    .output()
    .expect("spawn wb resume --rerun-step");
    assert!(
        out.status.success(),
        "rerun-step resume should complete\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("rerun_step"),
        "rerun navigation should be logged:\n{stderr}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("after-resume"), "stdout:\n{stdout}");
}

#[test]
fn browser_pause_then_goto_step_skips_with_kind_goto() {
    let id = "cov-pause-goto";
    let fx = pause_to_42(id);

    let out = resume_cmd(
        &fx.home,
        &fx.sidecar,
        &fx.artifacts,
        &fx.checkpoints,
        &fx.sentinel,
        id,
    )
    .args(["--goto-step", "after"])
    .output()
    .expect("spawn wb resume --goto-step");
    assert!(
        out.status.success(),
        "goto-step resume should complete\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    // The jumped-over browser slice is skipped as kind goto_step.
    assert!(
        stderr.contains("goto_step"),
        "goto skip should be logged:\n{stderr}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("after-resume"), "stdout:\n{stdout}");

    // The checkpoint records the goto skip (kind "goto").
    let ckpt: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fx.checkpoints.join(format!("{id}.json"))).unwrap(),
    )
    .unwrap();
    let body = serde_json::to_string(&ckpt).unwrap();
    assert!(
        body.contains("\"goto\""),
        "checkpoint should record a goto skip: {body}"
    );
}

// --- handshake / spawn error paths (must fail cleanly, never hang) --------

#[test]
fn sidecar_exits_immediately_clean_error() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let sidecar = write_fake_sidecar(dir.path());
    let artifacts = dir.path().join("artifacts");
    let wb = dir.path().join("simple.md");
    let events = dir.path().join("events.jsonl");
    write(&wb, SIMPLE_BROWSER_WB);

    let out = base_run(&home, &sidecar, &artifacts, &wb, &events)
        .arg("--bail")
        .env("WB_FAKE_EXIT_ON_HELLO", "1")
        .output()
        .expect("spawn wb");
    assert!(
        !out.status.success(),
        "a sidecar that exits during handshake should fail the run"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.to_lowercase().contains("sidecar")
            || stderr.to_lowercase().contains("handshake")
            || stderr.contains("ready"),
        "expected a clean sidecar error:\n{stderr}"
    );
}

#[test]
fn sidecar_bad_handshake_reply_clean_error() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let sidecar = write_fake_sidecar(dir.path());
    let artifacts = dir.path().join("artifacts");
    let wb = dir.path().join("simple.md");
    let events = dir.path().join("events.jsonl");
    write(&wb, SIMPLE_BROWSER_WB);

    let out = base_run(&home, &sidecar, &artifacts, &wb, &events)
        .arg("--bail")
        .env("WB_FAKE_BAD_READY", "1")
        .output()
        .expect("spawn wb");
    assert!(
        !out.status.success(),
        "a sidecar that never replies `ready` should fail the run"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.to_lowercase().contains("sidecar"),
        "expected a sidecar error:\n{stderr}"
    );
}

// --- vendor mismatch: runbook browser_service vs WB_BROWSER_VENDOR ---------

#[test]
fn browser_vendor_mismatch_clean_error() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let sidecar = write_fake_sidecar(dir.path());
    let artifacts = dir.path().join("artifacts");
    let wb = dir.path().join("vendor.md");
    let events = dir.path().join("events.jsonl");
    write(
        &wb,
        r#"---
title: Browser vendor mismatch
vars:
  browser_service: alpha
env:
  WB_BROWSER_VENDOR: beta
---
```browser {#nav}
session: cov
verbs:
  - goto: https://example.com
```
"#,
    );

    let out = base_run(&home, &sidecar, &artifacts, &wb, &events)
        .arg("--bail")
        .output()
        .expect("spawn wb");
    assert!(
        !out.status.success(),
        "a vendor mismatch should fail the run"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("vendor mismatch"),
        "expected a vendor mismatch error:\n{stderr}"
    );
}
