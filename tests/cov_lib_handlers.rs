//! Integration coverage for reachable `src/lib.rs` handler functions, driven by
//! spawning the real `wb` binary so llvm-cov instruments the run. These tests
//! target GAPS left by the existing `cov_*.rs` / `cli_smoke.rs` /
//! `orchestration_contract.rs` suites:
//!
//!   * `inspect_workbook` / `inspect_workbook_json` over diverse workbooks
//!     (wait / browser / http+sql / expect / required-includes / no-run flags /
//!     explicit + auto step ids) so every render branch fires.
//!   * `cmd_resume_cmd` navigation + error branches not covered by cov_browser
//!     (explicit rerun-step id, backward goto, action-in-signal, and the
//!     unknown-id / non-paused / unknown-goto-target usage errors).
//!   * `cmd_trust` (remove / remove-absent / json formats / untrusted-vs-changed).
//!   * `cmd_config` (every known key, json on each, unknown-key + unset paths).
//!   * `cmd_watch` / `render_watch` non-serve snapshots over completed / failed /
//!     paused checkpoints (text + `--format json` + `--once`).
//!   * `dry_run_preview` conditional reasons (`when` / `skip_if`).
//!   * `run_folder` mixed pass/fail aggregation.
//!   * `run_setup` ordering (setup runs before blocks) + setup-failure abort.
//!   * `walk_expects` over a multi-assertion workbook exercising every operator.
//!
//! No real network, docker, redis, or real browser: browser pauses use the same
//! fake `#!/bin/sh` sidecar pattern as cov_browser / orchestration_contract.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn wb_binary() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("wb")
}

fn have(tool: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {tool}")])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn out_str(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}
fn err_str(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}
fn both(o: &std::process::Output) -> String {
    format!("stdout:\n{}\nstderr:\n{}", out_str(o), err_str(o))
}
fn code(o: &std::process::Output) -> i32 {
    o.status.code().unwrap_or(-1)
}

fn write(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
}

/// `wb test --format json` runs blocks live, so block stdout can precede the
/// JSON object. Slice from the first '{' (our test blocks emit no braces).
fn json_tail(stdout: &str) -> serde_json::Value {
    let start = stdout.find('{').expect("a json object in stdout");
    serde_json::from_str(&stdout[start..]).expect("parseable json tail")
}

// ───────────────────────────── inspect_workbook (+ json) ────────────────────
//
// Existing cov_subcommands covers step-ids / sandbox / include. These fill the
// remaining render branches: wait, browser, http+sql, expect, required, flags.

#[test]
fn inspect_wait_workbook_text_and_json() {
    // Wait section renders the "⏸ wait …" line (text) and a kind:"wait" block
    // with kind_name/bind/timeout/on_timeout (json).
    let text = Command::new(wb_binary())
        .args(["inspect", "examples/wait-demo.md"])
        .output()
        .expect("spawn wb");
    assert_eq!(code(&text), 0, "{}", both(&text));
    let t = out_str(&text);
    assert!(t.contains("wait"), "text inspect should mention wait:\n{t}");

    let json = Command::new(wb_binary())
        .args(["inspect", "examples/wait-demo.md", "--json"])
        .output()
        .expect("spawn wb");
    assert_eq!(code(&json), 0, "{}", both(&json));
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).expect("inspect --json");
    let wait = v["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["kind"] == "wait")
        .expect("a wait block");
    assert_eq!(wait["kind_name"], "manual");
    assert_eq!(wait["bind"], "otp_code");
    assert_eq!(wait["on_timeout"], "abort");
}

#[test]
fn inspect_browser_workbook_text_and_json() {
    let text = Command::new(wb_binary())
        .args(["inspect", "examples/browser-demo.md"])
        .output()
        .expect("spawn wb");
    assert_eq!(code(&text), 0, "{}", both(&text));
    assert!(
        out_str(&text).contains("[browser]"),
        "text inspect should tag a browser slice:\n{}",
        out_str(&text)
    );

    let json = Command::new(wb_binary())
        .args(["inspect", "examples/browser-demo.md", "--json"])
        .output()
        .expect("spawn wb");
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).expect("inspect --json");
    let browser = v["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["kind"] == "browser")
        .expect("a browser block");
    assert_eq!(browser["session"], "ipostal1");
    assert!(browser["verb_count"].as_u64().unwrap() >= 1);
}

#[test]
fn inspect_required_includes_emit_frames_in_json() {
    // required: prepends a synthetic include → include_enter/include_exit frames.
    let json = Command::new(wb_binary())
        .args(["inspect", "examples/required-demo.md", "--json"])
        .output()
        .expect("spawn wb");
    assert_eq!(code(&json), 0, "{}", both(&json));
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).expect("inspect --json");
    let kinds: Vec<&str> = v["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|b| b["kind"].as_str())
        .collect();
    assert!(
        kinds.contains(&"include_enter"),
        "required prerequisite should splice an include_enter:\n{kinds:?}"
    );
    assert!(
        kinds.contains(&"include_exit"),
        "and a matching include_exit:\n{kinds:?}"
    );
}

#[test]
fn inspect_expect_fences_count_as_expect_blocks() {
    let json = Command::new(wb_binary())
        .args(["inspect", "examples/test-demo.md", "--json"])
        .output()
        .expect("spawn wb");
    assert_eq!(code(&json), 0, "{}", both(&json));
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).expect("inspect --json");
    let expect_block = v["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["kind"] == "expect")
        .expect("an expect block");
    assert!(expect_block["assertions"].as_u64().unwrap() >= 1);
}

#[test]
fn inspect_http_and_sql_and_step_ids_text_and_json() {
    let dir = tempfile::tempdir().unwrap();
    let wb = dir.path().join("mixed.md");
    write(
        &wb,
        r#"---
title: Mixed runtimes
runtime: bash
---

```http {#health}
GET https://example.com/health
Accept: application/json
```

```sql {#query timeout=30s retries=2}
SELECT 1;
```

```bash {continue_on_error}
echo auto-id-block
```
"#,
    );

    let text = Command::new(wb_binary())
        .args(["inspect", wb.to_str().unwrap()])
        .output()
        .expect("spawn wb");
    assert_eq!(code(&text), 0, "{}", both(&text));
    let t = out_str(&text);
    assert!(
        t.contains("title: Mixed runtimes"),
        "frontmatter title:\n{t}"
    );
    assert!(t.contains("[http]"), "http block tag:\n{t}");
    assert!(t.contains("[sql]"), "sql block tag:\n{t}");

    let json = Command::new(wb_binary())
        .args(["inspect", wb.to_str().unwrap(), "--json"])
        .output()
        .expect("spawn wb");
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).expect("inspect --json");
    let blocks = v["blocks"].as_array().unwrap();
    let ids: Vec<String> = blocks
        .iter()
        .filter_map(|b| b["step_id"].as_str().map(String::from))
        .collect();
    assert!(
        ids.contains(&"health".to_string()),
        "explicit id health:\n{ids:?}"
    );
    assert!(
        ids.iter().any(|id| id.starts_with("auto-")),
        "the un-id'd block gets an auto-<hex> id:\n{ids:?}"
    );
    let langs: Vec<&str> = blocks
        .iter()
        .filter_map(|b| b["language"].as_str())
        .collect();
    assert!(langs.contains(&"http"));
    assert!(langs.contains(&"sql"));
    assert_eq!(v["executable_count"].as_u64().unwrap(), 3);
}

#[test]
fn inspect_no_run_flag_annotation_renders() {
    // http-demo carries a `{no-run}` http block → flag_annotation prints {no-run}.
    let text = Command::new(wb_binary())
        .args(["inspect", "examples/http-demo.md"])
        .output()
        .expect("spawn wb");
    assert_eq!(code(&text), 0, "{}", both(&text));
    assert!(
        out_str(&text).contains("{no-run}"),
        "no-run badge should render:\n{}",
        out_str(&text)
    );
}

#[test]
fn inspect_params_workbook_parses_blocks() {
    // params aren't surfaced by inspect, but the workbook must parse and report
    // its executable blocks (exercises the params-frontmatter parse path).
    let json = Command::new(wb_binary())
        .args(["inspect", "examples/params-demo.md", "--json"])
        .output()
        .expect("spawn wb");
    assert_eq!(code(&json), 0, "{}", both(&json));
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).expect("inspect --json");
    assert!(v["executable_count"].as_u64().unwrap() >= 1);
    assert_eq!(v["source"], "examples/params-demo.md");
}

#[test]
fn inspect_missing_file_exits_one() {
    let o = Command::new(wb_binary())
        .args(["inspect", "examples/does-not-exist-xyz.md"])
        .output()
        .expect("spawn wb");
    assert_eq!(
        code(&o),
        1,
        "missing file inspect should exit 1:\n{}",
        both(&o)
    );
}

// ───────────────────────────── cmd_trust ───────────────────────────────────

fn trust_wb(home: &Path, args: &[&str]) -> std::process::Output {
    Command::new(wb_binary())
        .args(args)
        .env("HOME", home)
        .output()
        .expect("spawn wb")
}

#[test]
fn trust_add_list_remove_json_lifecycle() {
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let wb = dir.path().join("t.md");
    write(&wb, "```bash\necho hi\n```\n");
    let f = wb.to_str().unwrap();

    // add --format json
    let add = trust_wb(home.path(), &["trust", "add", f, "--format", "json"]);
    assert_eq!(code(&add), 0, "{}", both(&add));
    let v: serde_json::Value = serde_json::from_slice(&add.stdout).expect("add json");
    assert_eq!(v["ok"], true);
    assert!(v["sha256"].as_str().unwrap().len() >= 12);

    // list --format json contains the entry
    let list = trust_wb(home.path(), &["trust", "list", "--format", "json"]);
    assert_eq!(code(&list), 0, "{}", both(&list));
    let lv: serde_json::Value = serde_json::from_slice(&list.stdout).expect("list json");
    // The store canonicalizes paths (macOS /var → /private/var), so match by
    // file-name suffix rather than the raw arg.
    assert!(
        lv["entries"].as_array().unwrap().iter().any(|e| e["file"]
            .as_str()
            .map(|s| s.ends_with("t.md"))
            .unwrap_or(false)),
        "list should include the trusted file:\n{}",
        out_str(&list)
    );

    // check trusted → exit 0
    let check = trust_wb(home.path(), &["trust", "check", f, "--format", "json"]);
    assert_eq!(code(&check), 0, "{}", both(&check));
    let cv: serde_json::Value = serde_json::from_slice(&check.stdout).expect("check json");
    assert_eq!(cv["status"], "trusted");

    // remove --format json → ok true
    let rm = trust_wb(home.path(), &["trust", "remove", f, "--format", "json"]);
    assert_eq!(code(&rm), 0, "{}", both(&rm));
    let rv: serde_json::Value = serde_json::from_slice(&rm.stdout).expect("remove json");
    assert_eq!(rv["ok"], true);

    // removing again → ok false (absent)
    let rm2 = trust_wb(home.path(), &["trust", "remove", f, "--format", "json"]);
    assert_eq!(code(&rm2), 0, "{}", both(&rm2));
    let rv2: serde_json::Value = serde_json::from_slice(&rm2.stdout).expect("remove json");
    assert_eq!(rv2["ok"], false, "second remove reports nothing removed");
}

#[test]
fn trust_check_untrusted_then_changed() {
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let wb = dir.path().join("c.md");
    write(&wb, "```bash\necho one\n```\n");
    let f = wb.to_str().unwrap();

    // untrusted → non-zero usage exit, label "untrusted"
    let untrusted = trust_wb(home.path(), &["trust", "check", f]);
    assert_eq!(
        code(&untrusted),
        2,
        "untrusted check is a usage exit:\n{}",
        both(&untrusted)
    );
    assert!(out_str(&untrusted).contains("untrusted"));

    // trust it, then mutate the file → "changed"
    assert_eq!(code(&trust_wb(home.path(), &["trust", "add", f])), 0);
    write(&wb, "```bash\necho TWO\n```\n");
    let changed = trust_wb(home.path(), &["trust", "check", f, "--format", "json"]);
    assert_eq!(
        code(&changed),
        2,
        "changed file is non-trusted:\n{}",
        both(&changed)
    );
    let cv: serde_json::Value = serde_json::from_slice(&changed.stdout).expect("check json");
    assert_eq!(cv["status"], "changed");
}

#[test]
fn trust_list_empty_is_friendly() {
    let home = tempfile::tempdir().unwrap();
    let list = trust_wb(home.path(), &["trust", "list"]);
    assert_eq!(code(&list), 0, "{}", both(&list));
    assert!(
        out_str(&list).contains("no trusted workbooks"),
        "empty store message:\n{}",
        out_str(&list)
    );
}

// ───────────────────────────── cmd_config ──────────────────────────────────

fn cfg(home: &Path, args: &[&str]) -> std::process::Output {
    Command::new(wb_binary())
        .args(args)
        .env("HOME", home)
        .output()
        .expect("spawn wb")
}

#[test]
fn config_each_known_key_set_get_unset() {
    let home = tempfile::tempdir().unwrap();
    for (key, val) in [
        ("callback.url", "https://hooks.example.com/wb"),
        ("callback.secret", "shhh"),
        ("callback.key", "wb:stream:events"),
    ] {
        let set = cfg(
            home.path(),
            &["config", "set", key, val, "--format", "json"],
        );
        assert_eq!(code(&set), 0, "set {key}:\n{}", both(&set));
        let sv: serde_json::Value = serde_json::from_slice(&set.stdout).expect("set json");
        assert_eq!(sv["ok"], true);
        assert_eq!(sv["value"], val);

        let get = cfg(home.path(), &["config", "get", key, "--format", "json"]);
        assert_eq!(code(&get), 0, "get {key}:\n{}", both(&get));
        let gv: serde_json::Value = serde_json::from_slice(&get.stdout).expect("get json");
        assert_eq!(gv["value"], val);

        let unset = cfg(home.path(), &["config", "unset", key, "--format", "json"]);
        assert_eq!(code(&unset), 0, "unset {key}:\n{}", both(&unset));
        let uv: serde_json::Value = serde_json::from_slice(&unset.stdout).expect("unset json");
        assert_eq!(uv["removed"], true);
    }
}

#[test]
fn config_path_and_list_json() {
    let home = tempfile::tempdir().unwrap();
    let path = cfg(home.path(), &["config", "path", "--format", "json"]);
    assert_eq!(code(&path), 0, "{}", both(&path));
    let pv: serde_json::Value = serde_json::from_slice(&path.stdout).expect("path json");
    assert!(pv["path"].as_str().unwrap().ends_with("config.yaml"));

    // list json on an empty store still lists known_keys.
    let list = cfg(home.path(), &["config", "list", "--format", "json"]);
    assert_eq!(code(&list), 0, "{}", both(&list));
    let lv: serde_json::Value = serde_json::from_slice(&list.stdout).expect("list json");
    let known: Vec<&str> = lv["known_keys"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|k| k["key"].as_str())
        .collect();
    assert!(known.contains(&"callback.url"));
    assert!(known.contains(&"callback.secret"));
    assert!(known.contains(&"callback.key"));
}

#[test]
fn config_unknown_key_set_rejected() {
    let home = tempfile::tempdir().unwrap();
    let o = cfg(home.path(), &["config", "set", "bogus.key.xyz", "v"]);
    assert_eq!(
        code(&o),
        2,
        "unknown key set is a usage error:\n{}",
        both(&o)
    );
}

#[test]
fn config_get_unset_key_exits_two_but_emits_json_null() {
    let home = tempfile::tempdir().unwrap();
    let o = cfg(
        home.path(),
        &["config", "get", "callback.url", "--format", "json"],
    );
    assert_eq!(code(&o), 2, "unset get exits 2:\n{}", both(&o));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).expect("get json");
    assert!(
        v["value"].is_null(),
        "value should be null:\n{}",
        out_str(&o)
    );
}

#[test]
fn config_unset_absent_key_reports_not_removed() {
    let home = tempfile::tempdir().unwrap();
    let o = cfg(
        home.path(),
        &["config", "unset", "callback.key", "--format", "json"],
    );
    assert_eq!(code(&o), 0, "{}", both(&o));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).expect("unset json");
    assert_eq!(v["removed"], false, "nothing to remove:\n{}", out_str(&o));
}

// ───────────────────────────── cmd_watch / render_watch ────────────────────
//
// Non-serve snapshots only. `--once` and `--format json` are one-shot (the live
// loop blocks and is deliberately not exercised here).

/// Run a workbook to a checkpointed state under an isolated HOME + checkpoint
/// dir. Returns (home, checkpoint_dir) kept alive by the caller's TempDirs.
fn run_checkpointed(
    home: &Path,
    ckpt_dir: &Path,
    workbook: &Path,
    id: &str,
    bail: bool,
) -> std::process::Output {
    let mut cmd = Command::new(wb_binary());
    cmd.args(["run", workbook.to_str().unwrap(), "--checkpoint", id]);
    if bail {
        cmd.arg("--bail");
    }
    cmd.env("HOME", home)
        .env("WB_CHECKPOINT_DIR", ckpt_dir)
        .env("WB_LOG_LEVEL", "error")
        .output()
        .expect("spawn wb run")
}

fn watch(home: &Path, ckpt_dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(wb_binary())
        .args(args)
        .env("HOME", home)
        .env("WB_CHECKPOINT_DIR", ckpt_dir)
        .output()
        .expect("spawn wb watch")
}

#[test]
fn watch_completed_run_text_and_json_snapshot() {
    let home = tempfile::tempdir().unwrap();
    let ckpt = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let wb = dir.path().join("done.md");
    write(
        &wb,
        "```bash\necho block-one\n```\n```bash\necho block-two\n```\n",
    );
    let id = "watch-done";
    let run = run_checkpointed(home.path(), ckpt.path(), &wb, id, false);
    assert_eq!(code(&run), 0, "run should complete:\n{}", both(&run));

    // text snapshot (--once)
    let text = watch(home.path(), ckpt.path(), &["watch", id, "--once"]);
    assert_eq!(code(&text), 0, "{}", both(&text));
    let t = out_str(&text);
    assert!(t.contains("watch watch-done"), "header:\n{t}");
    assert!(t.contains("complete"), "status complete:\n{t}");
    assert!(t.contains("2 ok"), "two passing blocks:\n{t}");

    // json snapshot
    let json = watch(home.path(), ckpt.path(), &["watch", id, "--format", "json"]);
    assert_eq!(code(&json), 0, "{}", both(&json));
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).expect("watch json");
    assert_eq!(v["checkpoint"], id);
    assert_eq!(v["status"], "complete");
    assert_eq!(v["passed"], 2);
    assert_eq!(v["total_blocks"], 2);
    assert_eq!(v["results"].as_array().unwrap().len(), 2);
}

#[test]
fn watch_failed_run_snapshot() {
    let home = tempfile::tempdir().unwrap();
    let ckpt = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let wb = dir.path().join("fail.md");
    write(&wb, "```bash\necho ok\n```\n```bash\nexit 3\n```\n");
    let id = "watch-fail";
    let run = run_checkpointed(home.path(), ckpt.path(), &wb, id, true);
    assert_ne!(code(&run), 0, "bail run should fail:\n{}", both(&run));

    let json = watch(home.path(), ckpt.path(), &["watch", id, "--format", "json"]);
    assert_eq!(code(&json), 0, "{}", both(&json));
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).expect("watch json");
    assert_eq!(v["status"], "failed");
    // A --bail run records the completed prefix; the snapshot reports the
    // failed status even though the bailed block isn't a passing result.
    assert!(
        v["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["exit_code"] == 0),
        "the passing prefix block is recorded:\n{}",
        out_str(&json)
    );

    // text path covers render_watch's ✗ branch.
    let text = watch(home.path(), ckpt.path(), &["watch", id, "--once"]);
    assert!(out_str(&text).contains("failed"), "{}", both(&text));
}

#[test]
fn watch_paused_run_shows_pending_wait() {
    let home = tempfile::tempdir().unwrap();
    let ckpt = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let wb = dir.path().join("pause.md");
    write(
        &wb,
        "```bash\necho before\n```\n```wait\nkind: manual\nbind: code\ntimeout: 1h\non_timeout: abort\n```\n```bash\necho after\n```\n",
    );
    let id = "watch-pause";
    let run = run_checkpointed(home.path(), ckpt.path(), &wb, id, false);
    assert_eq!(
        code(&run),
        42,
        "wait run should pause (42):\n{}",
        both(&run)
    );

    let json = watch(home.path(), ckpt.path(), &["watch", id, "--format", "json"]);
    assert_eq!(code(&json), 0, "{}", both(&json));
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).expect("watch json");
    assert_eq!(v["status"], "paused");
    assert_eq!(v["pending"]["kind"], "manual");

    // render_watch's pending branch prints the ⏸ line.
    let text = watch(home.path(), ckpt.path(), &["watch", id, "--once"]);
    assert!(
        out_str(&text).contains("waiting at"),
        "pending wait should render:\n{}",
        out_str(&text)
    );
}

#[test]
fn watch_unknown_checkpoint_is_usage_error() {
    let home = tempfile::tempdir().unwrap();
    let ckpt = tempfile::tempdir().unwrap();
    let o = watch(home.path(), ckpt.path(), &["watch", "nope-xyz", "--once"]);
    assert_eq!(
        code(&o),
        2,
        "missing checkpoint is a usage error:\n{}",
        both(&o)
    );
    assert!(err_str(&o).contains("no checkpoint"));
}

// ───────────────────────────── dry_run_preview ─────────────────────────────
//
// cov_selection_cache covers profile/param/selection/no-run plan rows. Gap:
// the conditional reasons (when / skip_if).

#[test]
fn dry_run_marks_conditional_skips_with_reason() {
    let dir = tempfile::tempdir().unwrap();
    let wb = dir.path().join("cond.md");
    write(
        &wb,
        r#"---
runtime: bash
env:
  DEPLOY_ENV: staging
---
```bash
echo always
```
```bash {when=$DEPLOY_ENV=prod}
echo prod-only
```
```bash {skip_if=$DEPLOY_ENV}
echo skipped-because-truthy
```
"#,
    );
    let o = Command::new(wb_binary())
        .args(["run", wb.to_str().unwrap(), "--dry-run"])
        .env("HOME", dir.path())
        .output()
        .expect("spawn wb");
    assert_eq!(code(&o), 0, "{}", both(&o));
    let t = out_str(&o);
    // Block 1 runs; block 2 skipped (when false); block 3 skipped (skip_if true).
    assert!(t.contains("run "), "an always-run row:\n{t}");
    assert!(t.contains("skip"), "conditional skips present:\n{t}");
    assert!(t.contains("when"), "a when-reason should be shown:\n{t}");
    assert!(
        t.contains("skip_if"),
        "a skip_if-reason should be shown:\n{t}"
    );
    assert!(
        t.contains("1 would run, 2 skipped"),
        "plan summary should count 1 run / 2 skipped:\n{t}"
    );
}

// ───────────────────────────── run_folder ──────────────────────────────────

#[test]
fn run_folder_mixed_pass_fail_reports_failure_and_aggregates() {
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("a-pass.md"),
        "```bash\necho PASS_TOKEN\n```\n",
    );
    write(&dir.path().join("b-fail.md"), "```bash\nexit 4\n```\n");
    let report = home.path().join("agg.md");

    let o = Command::new(wb_binary())
        .arg(dir.path())
        .args(["-q", "-o"])
        .arg(&report)
        .env("HOME", home.path())
        .output()
        .expect("spawn wb");
    assert_ne!(
        code(&o),
        0,
        "a folder with a failing workbook should exit non-zero:\n{}",
        both(&o)
    );
    let written = std::fs::read_to_string(&report).expect("aggregate report written");
    assert!(
        written.contains("a-pass.md"),
        "report lists pass file:\n{written}"
    );
    assert!(
        written.contains("b-fail.md"),
        "report lists fail file:\n{written}"
    );
    assert!(
        written.contains("Ran 2 workbooks"),
        "run summary:\n{written}"
    );
}

// ───────────────────────────── run_setup ───────────────────────────────────

#[test]
fn setup_runs_before_blocks() {
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("marker.txt");
    let wb = dir.path().join("setup.md");
    // setup writes a marker; the block reads it back, proving ordering.
    write(
        &wb,
        &format!(
            "---\nsetup:\n  - echo setup-ran > {m}\n---\n```bash\ncat {m}\necho block-ran\n```\n",
            m = marker.to_str().unwrap()
        ),
    );
    let o = Command::new(wb_binary())
        .args(["run", wb.to_str().unwrap()])
        .env("HOME", home.path())
        .output()
        .expect("spawn wb");
    assert_eq!(code(&o), 0, "{}", both(&o));
    let t = out_str(&o);
    assert!(t.contains("setup-ran"), "block read the setup marker:\n{t}");
    assert!(t.contains("block-ran"), "block ran:\n{t}");
}

#[test]
fn setup_failure_aborts_before_blocks() {
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let wb = dir.path().join("setup-fail.md");
    write(
        &wb,
        "---\nsetup:\n  - exit 9\n---\n```bash\necho SHOULD_NOT_RUN\n```\n",
    );
    let o = Command::new(wb_binary())
        .args(["run", wb.to_str().unwrap()])
        .env("HOME", home.path())
        .output()
        .expect("spawn wb");
    assert_ne!(
        code(&o),
        0,
        "failing setup should abort the run:\n{}",
        both(&o)
    );
    assert!(
        !out_str(&o).contains("SHOULD_NOT_RUN"),
        "blocks must not run after a failed setup:\n{}",
        out_str(&o)
    );
}

#[test]
fn setup_demo_example_runs() {
    let home = tempfile::tempdir().unwrap();
    let o = Command::new(wb_binary())
        .args(["run", "examples/setup-demo.md"])
        .env("HOME", home.path())
        .output()
        .expect("spawn wb");
    assert_eq!(code(&o), 0, "{}", both(&o));
    let t = out_str(&o);
    assert!(
        t.contains("installing dependencies"),
        "setup output present:\n{t}"
    );
    assert!(
        t.contains("code blocks run after setup completes"),
        "block ran:\n{t}"
    );
}

// ───────────────────────────── walk_expects ────────────────────────────────

#[test]
fn test_multi_assertion_every_operator_passes() {
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let wb = dir.path().join("asserts.md");
    write(
        &wb,
        r#"---
runtime: bash
---
```bash
echo "hello world"
echo "to stderr" 1>&2
( exit 5 )
```
```expect
exit 5
exit != 0
stdout contains "hello"
stdout not-contains "goodbye"
stdout equals "hello world"
stdout not-empty
stderr contains "stderr"
```
"#,
    );
    let o = Command::new(wb_binary())
        .args(["test", wb.to_str().unwrap(), "-q"])
        .env("HOME", home.path())
        .output()
        .expect("spawn wb");
    assert_eq!(code(&o), 0, "all assertions should pass:\n{}", both(&o));

    // json report shows the pass.
    let j = Command::new(wb_binary())
        .args(["test", wb.to_str().unwrap(), "--format", "json"])
        .env("HOME", home.path())
        .output()
        .expect("spawn wb");
    assert_eq!(code(&j), 0, "{}", both(&j));
    let v = json_tail(&out_str(&j));
    assert_eq!(v["ok"], true);
    assert!(
        v["passed"].as_u64().unwrap() >= 7,
        "all 7 operators counted:\n{}",
        out_str(&j)
    );
}

#[test]
fn test_failing_assertion_reports_operator_failure() {
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let wb = dir.path().join("fail-assert.md");
    write(
        &wb,
        "```bash\necho actual\n```\n```expect\nstdout equals \"expected\"\nstdout empty\n```\n",
    );
    let o = Command::new(wb_binary())
        .args(["test", wb.to_str().unwrap(), "--format", "json"])
        .env("HOME", home.path())
        .output()
        .expect("spawn wb");
    assert_eq!(code(&o), 1, "a failing assertion exits 1:\n{}", both(&o));
    let v = json_tail(&out_str(&o));
    assert_eq!(v["ok"], false);
    assert!(v["failed"].as_u64().unwrap() >= 1);
}

#[test]
fn verify_doc_with_passing_blocks_succeeds() {
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let doc = dir.path().join("README.md");
    write(
        &doc,
        "# Doc\n\nSome prose.\n\n```bash\necho doc-block-ran\n```\n\n```expect\nexit 0\nstdout contains \"doc-block\"\n```\n",
    );
    let o = Command::new(wb_binary())
        .args(["verify", doc.to_str().unwrap()])
        .env("HOME", home.path())
        .output()
        .expect("spawn wb");
    assert_eq!(code(&o), 0, "verify should pass:\n{}", both(&o));
}

// ───────────────────────────── cmd_resume_cmd: error branches ──────────────

#[test]
fn resume_unknown_checkpoint_exits_one() {
    let home = tempfile::tempdir().unwrap();
    let ckpt = tempfile::tempdir().unwrap();
    let o = Command::new(wb_binary())
        .args(["resume", "no-such-run", "--value", "x"])
        .env("HOME", home.path())
        .env("WB_CHECKPOINT_DIR", ckpt.path())
        .output()
        .expect("spawn wb");
    assert_eq!(
        code(&o),
        1,
        "resume of an unknown id exits 1:\n{}",
        both(&o)
    );
    assert!(err_str(&o).contains("no checkpoint"));
}

#[test]
fn resume_completed_checkpoint_is_invalid() {
    let home = tempfile::tempdir().unwrap();
    let ckpt = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let wb = dir.path().join("done.md");
    write(&wb, "```bash\necho done\n```\n");
    let id = "resume-completed";
    let run = run_checkpointed(home.path(), ckpt.path(), &wb, id, false);
    assert_eq!(code(&run), 0, "run should complete:\n{}", both(&run));

    let o = Command::new(wb_binary())
        .args(["resume", id, "--value", "x"])
        .env("HOME", home.path())
        .env("WB_CHECKPOINT_DIR", ckpt.path())
        .output()
        .expect("spawn wb");
    assert_eq!(
        code(&o),
        3,
        "resuming a completed checkpoint is EXIT_WORKBOOK_INVALID:\n{}",
        both(&o)
    );
    assert!(err_str(&o).contains("not paused"));
}

#[test]
fn resume_wait_missing_required_bind_errors() {
    let home = tempfile::tempdir().unwrap();
    let ckpt = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let wb = dir.path().join("wait.md");
    write(
        &wb,
        "```wait\nkind: manual\nbind: token\ntimeout: 1h\non_timeout: abort\n```\n```bash\necho got=$token\n```\n",
    );
    let id = "resume-missing-bind";
    let run = run_checkpointed(home.path(), ckpt.path(), &wb, id, false);
    assert_eq!(code(&run), 42, "should pause:\n{}", both(&run));

    // Resume with no --value / --signal → required bind unsatisfied → exit 1.
    let o = Command::new(wb_binary())
        .args(["resume", id])
        .env("HOME", home.path())
        .env("WB_CHECKPOINT_DIR", ckpt.path())
        .output()
        .expect("spawn wb");
    assert_eq!(code(&o), 1, "missing bind should exit 1:\n{}", both(&o));
    assert!(err_str(&o).contains("missing required bind"));
}

#[test]
fn resume_wait_with_signal_file_binds_value() {
    let home = tempfile::tempdir().unwrap();
    let ckpt = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let wb = dir.path().join("wait.md");
    write(
        &wb,
        "```wait\nkind: manual\nbind: token\ntimeout: 1h\non_timeout: abort\n```\n```bash\necho got=$token\n```\n",
    );
    let id = "resume-signal-file";
    assert_eq!(
        code(&run_checkpointed(home.path(), ckpt.path(), &wb, id, false)),
        42
    );

    let sig = dir.path().join("sig.json");
    write(&sig, "{\"token\": \"abc123\"}");
    let o = Command::new(wb_binary())
        .args(["resume", id, "--signal", sig.to_str().unwrap()])
        .env("HOME", home.path())
        .env("WB_CHECKPOINT_DIR", ckpt.path())
        .output()
        .expect("spawn wb");
    assert_eq!(code(&o), 0, "{}", both(&o));
    assert!(
        out_str(&o).contains("got=abc123"),
        "bound value:\n{}",
        out_str(&o)
    );
}

// ───────────────────────────── cmd_resume_cmd: browser navigation ──────────
//
// Reuses the fake-sidecar + sentinel pattern from cov_browser, but with a
// THREE-step workbook (prep / pause / after) so backward goto, explicit
// rerun-step ids, and action-in-signal navigation are exercised — branches the
// existing two-step cov_browser fixtures don't reach.

#[cfg(unix)]
const NAV_SIDECAR: &str = r#"#!/bin/sh
emit() { printf '%s\n' "$1"; }
while IFS= read -r line; do
  case "$line" in
    *'"type":"hello"'*)
      emit '{"type":"ready"}'
      ;;
    *'"type":"slice"'*)
      if [ -f "$WB_FAKE_SENTINEL" ]; then
        emit '{"type":"verb.complete","verb":"continue","summary":"resumed"}'
        emit '{"type":"slice.complete"}'
      else
        : > "$WB_FAKE_SENTINEL"
        emit '{"type":"slice.paused","reason":"pause_for_human","resume_url":"https://live.example/s","verb_index":0,"message":"Approve","resume_on":"operator_click","timeout":"30m","actions":[{"kind":"goto_step","target":"after","label":"Skip"}],"sidecar_state":{"session":"abc"}}'
      fi
      ;;
    *'"type":"suspend"'*|*'"type":"shutdown"'*)
      exit 0
      ;;
  esac
done
"#;

#[cfg(unix)]
const NAV_WB: &str = r#"---
title: Nav
---
## Prep
```bash {#prep}
echo "prep-ran"
```

## Pause
```browser {#pause}
session: nav
verbs:
  - pause_for_human:
      message: Approve
      resume_on: operator_click
```

## After
```bash {#after}
echo "after-ran"
```
"#;

#[cfg(unix)]
struct NavFixture {
    _dir: tempfile::TempDir,
    home: PathBuf,
    sidecar: PathBuf,
    checkpoints: PathBuf,
    sentinel: PathBuf,
    id: String,
}

#[cfg(unix)]
fn nav_pause(id: &str) -> NavFixture {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let sidecar = dir.path().join("nav-sidecar.sh");
    write(&sidecar, NAV_SIDECAR);
    let mut perms = std::fs::metadata(&sidecar).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&sidecar, perms).unwrap();
    let checkpoints = dir.path().join("checkpoints");
    let sentinel = dir.path().join("sentinel");
    let workbook = dir.path().join("nav.md");
    write(&workbook, NAV_WB);

    let out = Command::new(wb_binary())
        .args(["run", workbook.to_str().unwrap(), "--checkpoint", id])
        .env("HOME", &home)
        .env("WB_BROWSER_RUNTIME", &sidecar)
        .env("WB_CHECKPOINT_DIR", &checkpoints)
        .env("WB_FAKE_SENTINEL", &sentinel)
        .env("WB_SIDECAR_SHUTDOWN_TIMEOUT_SECS", "5")
        .env("WB_LOG_LEVEL", "warn")
        .output()
        .expect("spawn wb run");
    assert_eq!(
        code(&out),
        42,
        "nav workbook should pause at the browser slice:\n{}",
        both(&out)
    );
    assert!(
        out_str(&out).contains("prep-ran"),
        "prep ran before pause:\n{}",
        out_str(&out)
    );

    NavFixture {
        _dir: dir,
        home,
        sidecar,
        checkpoints,
        sentinel,
        id: id.to_string(),
    }
}

#[cfg(unix)]
fn nav_resume(fx: &NavFixture) -> Command {
    let mut cmd = Command::new(wb_binary());
    cmd.args(["resume", &fx.id])
        .env("HOME", &fx.home)
        .env("WB_BROWSER_RUNTIME", &fx.sidecar)
        .env("WB_CHECKPOINT_DIR", &fx.checkpoints)
        .env("WB_FAKE_SENTINEL", &fx.sentinel)
        .env("WB_SIDECAR_SHUTDOWN_TIMEOUT_SECS", "5")
        .env("WB_LOG_LEVEL", "warn");
    cmd
}

#[test]
#[cfg(unix)]
fn resume_browser_rerun_step_explicit_id() {
    let fx = nav_pause("nav-rerun-id");
    // Explicit rerun-step at a *later* id (after) jumps the cursor forward.
    let o = nav_resume(&fx)
        .args(["--rerun-step", "after"])
        .output()
        .expect("spawn wb resume");
    assert_eq!(
        code(&o),
        0,
        "rerun-step <id> should complete:\n{}",
        both(&o)
    );
    assert!(
        err_str(&o).contains("rerun_step"),
        "navigation logged:\n{}",
        err_str(&o)
    );
    assert!(
        err_str(&o).contains("after"),
        "target step logged:\n{}",
        err_str(&o)
    );
    assert!(
        out_str(&o).contains("after-ran"),
        "after block ran:\n{}",
        out_str(&o)
    );
}

#[test]
#[cfg(unix)]
fn resume_browser_goto_earlier_step_reruns_prefix() {
    let fx = nav_pause("nav-goto-back");
    // goto an *earlier* step (prep) → backward jump: prep reruns, then the
    // browser slice completes (sentinel now set), then after runs.
    let o = nav_resume(&fx)
        .args(["--goto-step", "prep"])
        .output()
        .expect("spawn wb resume");
    assert_eq!(code(&o), 0, "backward goto should complete:\n{}", both(&o));
    assert!(
        err_str(&o).contains("goto_step"),
        "navigation logged:\n{}",
        err_str(&o)
    );
    assert!(
        err_str(&o).contains("prep"),
        "target step logged:\n{}",
        err_str(&o)
    );
    let t = out_str(&o);
    assert!(t.contains("prep-ran"), "prep reran:\n{t}");
    assert!(t.contains("after-ran"), "after ran:\n{t}");
}

#[test]
#[cfg(unix)]
fn resume_browser_goto_unknown_step_is_usage_error() {
    let fx = nav_pause("nav-goto-unknown");
    let o = nav_resume(&fx)
        .args(["--goto-step", "does-not-exist"])
        .output()
        .expect("spawn wb resume");
    assert_eq!(
        code(&o),
        2,
        "unknown goto target is a usage error:\n{}",
        both(&o)
    );
    assert!(err_str(&o).contains("not found"), "{}", err_str(&o));
}

#[test]
#[cfg(unix)]
fn resume_browser_action_in_signal_drives_goto_and_emits_step_skipped() {
    let fx = nav_pause("nav-action-signal");
    let (url, rx) = start_sink();

    // Action object in the signal JSON (delivered on stdin) selects goto_step.
    let mut child = nav_resume(&fx)
        .args(["--signal", "-", "--callback", &url])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn wb resume");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"action":{"kind":"goto_step","target":"after"}}"#)
        .unwrap();
    let o = child.wait_with_output().expect("wait wb resume");
    assert_eq!(
        code(&o),
        0,
        "action-driven goto should complete:\n{}",
        both(&o)
    );
    assert!(
        out_str(&o).contains("after-ran"),
        "after ran:\n{}",
        out_str(&o)
    );

    // The jumped-over pause slice is reported as step.skipped (kind "goto").
    let events = drain_sink(&rx);
    let goto_skip = events
        .iter()
        .any(|e| e["event"] == "step.skipped" && e["skip"]["kind"] == "goto");
    assert!(
        goto_skip,
        "expected a step.skipped with skip.kind=goto, got events: {:?}",
        events
            .iter()
            .map(|e| e["event"].clone())
            .collect::<Vec<_>>()
    );
}

// ───────────────────────────── HTTP callback sink (loopback) ────────────────
//
// A tiny drain-based sink: accepts connections until a deadline and hands back
// every parsed JSON body. Used to assert resume forwards step.skipped(goto).

fn start_sink() -> (String, mpsc::Receiver<serde_json::Value>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind sink");
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((stream, _)) => {
                    if let Some(body) = read_body(stream) {
                        let _ = tx.send(body);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });
    (format!("http://127.0.0.1:{port}/hook"), rx)
}

fn read_body(mut stream: TcpStream) -> Option<serde_json::Value> {
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    let mut bytes = Vec::new();
    let mut buf = [0u8; 2048];
    let mut header_end = None;
    let mut content_length = None;
    loop {
        let n = stream.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        if header_end.is_none() {
            header_end = bytes.windows(4).position(|w| w == b"\r\n\r\n");
            if let Some(end) = header_end {
                let headers = String::from_utf8_lossy(&bytes[..end]).to_string();
                content_length = headers.lines().find_map(|l| {
                    let (k, v) = l.split_once(':')?;
                    if k.eq_ignore_ascii_case("content-length") {
                        v.trim().parse::<usize>().ok()
                    } else {
                        None
                    }
                });
            }
        }
        if let (Some(end), Some(len)) = (header_end, content_length) {
            if bytes.len() >= end + 4 + len {
                break;
            }
        }
    }
    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let _ = stream.shutdown(Shutdown::Both);
    let end = header_end?;
    let body_start = end + 4;
    let len = content_length.unwrap_or_else(|| bytes.len().saturating_sub(body_start));
    let body_end = body_start.saturating_add(len).min(bytes.len());
    serde_json::from_slice(&bytes[body_start..body_end]).ok()
}

fn drain_sink(rx: &mpsc::Receiver<serde_json::Value>) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    // Pull everything that arrives within a short settle window.
    while let Ok(v) = rx.recv_timeout(Duration::from_secs(3)) {
        out.push(v);
    }
    out
}

// ───────────────────────────── sql inspect smoke (gated) ───────────────────

#[test]
fn inspect_sql_block_resolves_when_sqlite_present() {
    if !have("sqlite3") {
        eprintln!("skip: sqlite3 not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let wb = dir.path().join("q.md");
    write(&wb, "```sql\nSELECT 42;\n```\n");
    let o = Command::new(wb_binary())
        .args(["inspect", wb.to_str().unwrap(), "--json"])
        .output()
        .expect("spawn wb");
    assert_eq!(code(&o), 0, "{}", both(&o));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["blocks"][0]["language"], "sql");
}
