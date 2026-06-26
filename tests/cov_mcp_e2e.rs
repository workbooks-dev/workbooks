//! Extended end-to-end coverage for `wb mcp` (the JSON-RPC-over-stdio server in
//! `src/mcp.rs`). The existing `tests/mcp_e2e.rs` drives ONE author→run→pause→
//! resume→read scenario; this file fans out across the full protocol surface and
//! every tool, plus the error/edge paths (unknown method, parse error, unknown
//! tool, missing args, unknown run id, failing/passing/paused runs, all three
//! resume shapes, validate/inspect, list_pending empty + populated,
//! get_run_events, and clean shutdown on stdin EOF).
//!
//! Harness notes:
//! - Each server is spawned with an isolated `HOME` *and* `WB_CHECKPOINT_DIR`
//!   under a fresh tempdir so checkpoint/pending/run state never leaks between
//!   tests (or between this file and the rest of the suite).
//! - All stdout reads are bounded by a background reader thread + `recv_timeout`
//!   so a misbehaving server can never hang the test binary.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use serde_json::{json, Value};

fn wb_binary() -> std::path::PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("wb")
}

/// Bound on any single response read. Generous: the child shells out to a debug
/// `wb` binary which can be slow under load, but we still never hang forever.
const READ_TIMEOUT: Duration = Duration::from_secs(60);

/// A live `wb mcp` server. stdin is owned for writing; stdout is drained by a
/// background thread into a channel so reads are always bounded.
struct McpClient {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<String>,
    next_id: i64,
    _tmp: tempfile::TempDir,
}

impl McpClient {
    fn start() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let ckpt_dir = tmp.path().join("checkpoints");
        std::fs::create_dir_all(&ckpt_dir).unwrap();

        let mut child = Command::new(wb_binary())
            .arg("mcp")
            // Isolate ALL run state: runs/artifacts under HOME, checkpoints +
            // pending descriptors under WB_CHECKPOINT_DIR.
            .env("HOME", tmp.path())
            .env("WB_CHECKPOINT_DIR", &ckpt_dir)
            .env("WB_LOG_LEVEL", "error")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn wb mcp");

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel::<String>();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => break, // server closed stdout (exited)
                    Ok(_) => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        McpClient {
            child,
            stdin: Some(stdin),
            rx,
            next_id: 0,
            _tmp: tmp,
        }
    }

    fn write_line(&mut self, s: &str) {
        let stdin = self.stdin.as_mut().expect("stdin open");
        writeln!(stdin, "{s}").unwrap();
        stdin.flush().unwrap();
    }

    /// Read exactly one response line (bounded). Skips blank lines.
    fn read_response(&mut self) -> Value {
        loop {
            match self.rx.recv_timeout(READ_TIMEOUT) {
                Ok(line) => {
                    let t = line.trim();
                    if t.is_empty() {
                        continue;
                    }
                    return serde_json::from_str(t)
                        .unwrap_or_else(|e| panic!("bad json response: {e}\nline: {line}"));
                }
                Err(RecvTimeoutError::Timeout) => panic!("timed out waiting for server response"),
                Err(RecvTimeoutError::Disconnected) => panic!("server closed stdout unexpectedly"),
            }
        }
    }

    /// Send a request, read the matching response, assert the id round-trips.
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.write_line(&serde_json::to_string(&msg).unwrap());
        let resp = self.read_response();
        assert_eq!(resp["id"], id, "response id mismatch: {resp}");
        resp
    }

    fn notify(&mut self, method: &str) {
        let msg = json!({ "jsonrpc": "2.0", "method": method });
        self.write_line(&serde_json::to_string(&msg).unwrap());
    }

    /// tools/call → assert success and return the parsed `structuredContent`.
    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        let resp = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );
        let result = &resp["result"];
        assert_eq!(
            result["isError"], false,
            "tool {name} returned error: {result}"
        );
        result["structuredContent"].clone()
    }

    /// tools/call expecting an `isError: true` tool result. Returns the text.
    fn call_tool_err(&mut self, name: &str, arguments: Value) -> String {
        let resp = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );
        let result = &resp["result"];
        assert_eq!(
            result["isError"], true,
            "tool {name} unexpectedly ok: {result}"
        );
        result["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string()
    }

    /// Close stdin and wait (bounded) for the server to exit; returns the code.
    fn finish(mut self) -> i32 {
        drop(self.stdin.take()); // EOF on the server's stdin → loop breaks
        for _ in 0..600 {
            match self.child.try_wait().unwrap() {
                Some(status) => return status.code().unwrap_or(-1),
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        }
        let _ = self.child.kill();
        panic!("server did not exit within 30s after stdin close");
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Belt-and-suspenders: tests that don't call finish() still clean up.
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ── tiny workbook fixtures ──────────────────────────────────────────────────

fn write_wb(dir: &std::path::Path, name: &str, body: &str) -> String {
    let p = dir.join(name);
    std::fs::write(&p, body).unwrap();
    p.to_str().unwrap().to_string()
}

const PASS_WB: &str = "# Pass\n\n```bash\necho hello-from-wb\n```\n";
const FAIL_WB: &str = "# Fail\n\n```bash\necho before-fail\nexit 1\n```\n";
const PAUSE_WB: &str = "# Pause\n\n```bash\necho pre-pause\n```\n\n```wait\nkind: manual\nbind: token\ntimeout: 5m\n```\n\n```bash\necho resumed-with: $token\n```\n";

// ════════════════════════════════════════════════════════════════════════════
// initialize / tools/list
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn initialize_with_clientinfo_and_version() {
    let mut mcp = McpClient::start();
    let init = mcp.request(
        "initialize",
        json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": { "name": "cov-test", "version": "1.2.3" }
        }),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "wb");
    // Known requested version is echoed back verbatim.
    assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(
        init["result"]["capabilities"]["tools"]["listChanged"],
        false
    );
    mcp.notify("notifications/initialized");
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn initialize_without_clientinfo_or_version_falls_back() {
    let mut mcp = McpClient::start();
    // No protocolVersion / clientInfo at all.
    let init = mcp.request("initialize", json!({}));
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert!(init["result"]["instructions"]
        .as_str()
        .unwrap()
        .contains("run_workbook"));
    // An unknown version falls back to the default too.
    let init2 = mcp.request("initialize", json!({ "protocolVersion": "1999-01-01" }));
    assert_eq!(init2["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn tools_list_exposes_every_tool_with_schema() {
    let mut mcp = McpClient::start();
    let tools = mcp.request("tools/list", json!({}));
    let arr = tools["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = arr.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for want in [
        "author_workbook",
        "run_workbook",
        "resume_workbook",
        "inspect_workbook",
        "validate_workbook",
        "list_pending",
        "get_run_events",
    ] {
        assert!(names.contains(&want), "missing tool {want}");
    }
    // Each tool carries an object inputSchema and a non-empty description.
    for t in arr {
        assert_eq!(
            t["inputSchema"]["type"], "object",
            "tool {} schema",
            t["name"]
        );
        assert!(!t["description"].as_str().unwrap().is_empty());
    }
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn ping_returns_empty_result() {
    let mut mcp = McpClient::start();
    let resp = mcp.request("ping", json!({}));
    assert!(resp["result"].is_object());
    assert!(resp.get("error").is_none());
    assert_eq!(mcp.finish(), 0);
}

// ════════════════════════════════════════════════════════════════════════════
// author_workbook
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn author_workbook_write_overwrite_and_missing_content() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("nested/flow.md");
    let path_s = path.to_str().unwrap().to_string();
    let mut mcp = McpClient::start();

    // First write creates parent dirs.
    let r = mcp.call_tool(
        "author_workbook",
        json!({ "path": path_s, "content": PASS_WB }),
    );
    assert_eq!(r["path"], path_s);
    assert_eq!(r["bytes_written"], PASS_WB.len());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), PASS_WB);

    // Overwrite WITHOUT the flag → error.
    let err = mcp.call_tool_err(
        "author_workbook",
        json!({ "path": path_s, "content": "new" }),
    );
    assert!(err.contains("already exists"), "got: {err}");

    // Overwrite WITH the flag → success.
    let r2 = mcp.call_tool(
        "author_workbook",
        json!({ "path": path_s, "content": "new-body", "overwrite": true }),
    );
    assert_eq!(r2["bytes_written"], "new-body".len());

    // Missing required `content` → error.
    let err2 = mcp.call_tool_err("author_workbook", json!({ "path": path_s }));
    assert!(
        err2.contains("missing required arg: content"),
        "got: {err2}"
    );

    assert_eq!(mcp.finish(), 0);
}

// ════════════════════════════════════════════════════════════════════════════
// run_workbook: pass / fail / pause
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn run_workbook_passing_completes() {
    let tmp = tempfile::tempdir().unwrap();
    let file = write_wb(tmp.path(), "pass.md", PASS_WB);
    let mut mcp = McpClient::start();
    let run = mcp.call_tool(
        "run_workbook",
        json!({ "file": file, "run_id": "pass-run" }),
    );
    assert_eq!(run["status"], "completed", "run: {run}");
    assert_eq!(run["run_id"], "pass-run");
    assert_eq!(run["exit_code"], 0);
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn run_workbook_failing_with_bail_reports_failed() {
    let tmp = tempfile::tempdir().unwrap();
    let file = write_wb(tmp.path(), "fail.md", FAIL_WB);
    let mut mcp = McpClient::start();
    let run = mcp.call_tool(
        "run_workbook",
        json!({ "file": file, "run_id": "fail-run", "bail": true }),
    );
    assert_eq!(run["status"], "failed", "run: {run}");
    assert_eq!(run["exit_code"], 1);
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn run_workbook_with_vars_and_dir() {
    // Exercise the `vars` (--set) and `dir` (-C) argv-building branches.
    let tmp = tempfile::tempdir().unwrap();
    let workdir = tmp.path().join("work");
    std::fs::create_dir_all(&workdir).unwrap();
    let file = write_wb(
        tmp.path(),
        "vars.md",
        "# Vars\n\n```bash\necho FOO is $FOO\n```\n",
    );
    let mut mcp = McpClient::start();
    let run = mcp.call_tool(
        "run_workbook",
        json!({
            "file": file,
            "run_id": "vars-run",
            "vars": { "FOO": "bar123" },
            "dir": workdir.to_str().unwrap(),
            "bail": true
        }),
    );
    assert_eq!(run["status"], "completed", "run: {run}");
    // Confirm the injected var reached the block via the durable timeline.
    let events = mcp.call_tool("get_run_events", json!({ "run_id": "vars-run" }));
    let stdout: String = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["stdout"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(stdout.contains("FOO is bar123"), "stdout: {stdout}");
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn run_workbook_pause_surfaces_elicitation() {
    let tmp = tempfile::tempdir().unwrap();
    let file = write_wb(tmp.path(), "pause.md", PAUSE_WB);
    let mut mcp = McpClient::start();
    let run = mcp.call_tool(
        "run_workbook",
        json!({ "file": file, "run_id": "pause-run" }),
    );
    assert_eq!(run["status"], "input_required", "run: {run}");
    assert_eq!(run["exit_code"], 42);
    assert_eq!(run["elicitation"]["bind"][0], "token");
    assert_eq!(
        run["elicitation"]["requestedSchema"]["properties"]["token"]["type"],
        "string"
    );
    assert_eq!(
        run["elicitation"]["requestedSchema"]["required"][0],
        "token"
    );
    assert_eq!(mcp.finish(), 0);
}

// ════════════════════════════════════════════════════════════════════════════
// resume_workbook: value / signal / action+goto / unknown
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn resume_with_value_completes_run() {
    let tmp = tempfile::tempdir().unwrap();
    let file = write_wb(tmp.path(), "pause.md", PAUSE_WB);
    let mut mcp = McpClient::start();
    let run = mcp.call_tool("run_workbook", json!({ "file": file, "run_id": "rv" }));
    assert_eq!(run["status"], "input_required");

    let resumed = mcp.call_tool(
        "resume_workbook",
        json!({ "run_id": "rv", "value": "s3cr3t" }),
    );
    assert_eq!(resumed["status"], "completed", "resume: {resumed}");

    let events = mcp.call_tool("get_run_events", json!({ "run_id": "rv" }));
    let stdout: String = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["stdout"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(stdout.contains("resumed-with: s3cr3t"), "stdout: {stdout}");
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn resume_with_signal_object_completes_run() {
    let tmp = tempfile::tempdir().unwrap();
    let file = write_wb(tmp.path(), "pause.md", PAUSE_WB);
    let mut mcp = McpClient::start();
    let run = mcp.call_tool("run_workbook", json!({ "file": file, "run_id": "rs" }));
    assert_eq!(run["status"], "input_required");

    // Full JSON signal payload (the bound var keyed explicitly).
    let resumed = mcp.call_tool(
        "resume_workbook",
        json!({ "run_id": "rs", "signal": { "token": "viaSignal" } }),
    );
    assert_eq!(resumed["status"], "completed", "resume: {resumed}");
    let events = mcp.call_tool("get_run_events", json!({ "run_id": "rs" }));
    let stdout: String = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["stdout"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        stdout.contains("resumed-with: viaSignal"),
        "stdout: {stdout}"
    );
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn resume_rejects_non_object_signal() {
    // `signal` present but not an object → validation error before any spawn.
    let mut mcp = McpClient::start();
    let err = mcp.call_tool_err(
        "resume_workbook",
        json!({ "run_id": "whatever", "signal": "not-an-object" }),
    );
    assert!(err.contains("signal must be a JSON object"), "got: {err}");
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn resume_with_rerun_step_after_pause() {
    // Drive the `rerun_step` argv branch. Re-running the currently-paused step
    // (empty string) runs the paused wait again → run pauses once more.
    let tmp = tempfile::tempdir().unwrap();
    let file = write_wb(tmp.path(), "pause.md", PAUSE_WB);
    let mut mcp = McpClient::start();
    let run = mcp.call_tool("run_workbook", json!({ "file": file, "run_id": "rr" }));
    assert_eq!(run["status"], "input_required");

    let resumed = mcp.call_tool(
        "resume_workbook",
        json!({ "run_id": "rr", "value": "x", "rerun_step": "" }),
    );
    // Either it re-pauses (input_required) or completes; both are non-error and
    // exercise the rerun-step argv path. Assert we got a recognized status.
    let status = resumed["status"].as_str().unwrap();
    assert!(
        matches!(
            status,
            "input_required" | "completed" | "failed" | "invalid"
        ),
        "unexpected status: {resumed}"
    );
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn resume_with_action_goto_payload() {
    // `action` (without `signal`) composes into the stdin signal payload and
    // takes the --signal argv branch. Target a non-existent step so it resolves
    // to a usage/invalid result without needing a browser slice — the point is
    // to exercise the action-payload code path, not the navigation semantics.
    let tmp = tempfile::tempdir().unwrap();
    let file = write_wb(tmp.path(), "pause.md", PAUSE_WB);
    let mut mcp = McpClient::start();
    let run = mcp.call_tool("run_workbook", json!({ "file": file, "run_id": "ra" }));
    assert_eq!(run["status"], "input_required");

    let resp = mcp.request(
        "tools/call",
        json!({
            "name": "resume_workbook",
            "arguments": {
                "run_id": "ra",
                "action": { "kind": "goto_step", "target": "no-such-step" }
            }
        }),
    );
    // Whether ok or isError, the server must return a well-formed tool result.
    let result = &resp["result"];
    assert!(result.get("content").is_some(), "result: {result}");
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn resume_unknown_run_id_is_error_status() {
    let mut mcp = McpClient::start();
    // resume on a non-existent checkpoint: wb resume exits non-zero; the tool
    // still returns a structured result (not a JSON-RPC error), with a non-
    // "completed" status.
    let run = mcp.call_tool(
        "resume_workbook",
        json!({ "run_id": "does-not-exist-xyz", "value": "v" }),
    );
    assert_ne!(run["status"], "completed", "resume: {run}");
    assert_ne!(run["exit_code"], 0);
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn resume_missing_run_id_is_error() {
    let mut mcp = McpClient::start();
    let err = mcp.call_tool_err("resume_workbook", json!({ "value": "v" }));
    assert!(err.contains("missing required arg: run_id"), "got: {err}");
    assert_eq!(mcp.finish(), 0);
}

// ════════════════════════════════════════════════════════════════════════════
// inspect / validate
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn inspect_workbook_returns_structure() {
    let tmp = tempfile::tempdir().unwrap();
    let file = write_wb(tmp.path(), "pass.md", PASS_WB);
    let mut mcp = McpClient::start();
    let inspected = mcp.call_tool("inspect_workbook", json!({ "file": file }));
    assert!(
        inspected.is_object() || inspected.is_array(),
        "inspect: {inspected}"
    );
    // Missing file arg → tool error.
    let err = mcp.call_tool_err("inspect_workbook", json!({}));
    assert!(err.contains("missing required arg: file"), "got: {err}");
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn validate_workbook_clean_and_with_diagnostics() {
    let tmp = tempfile::tempdir().unwrap();
    let good = write_wb(tmp.path(), "good.md", PASS_WB);
    // Duplicate explicit step ids → wb-step-001 validation error.
    let bad_body = "# Bad\n\n```bash {#dup}\necho a\n```\n\n```bash {#dup}\necho b\n```\n";
    let bad = write_wb(tmp.path(), "bad.md", bad_body);
    let mut mcp = McpClient::start();

    let clean = mcp.call_tool("validate_workbook", json!({ "file": good }));
    assert_eq!(clean["valid"], true, "clean validate: {clean}");
    assert!(clean.get("diagnostics").is_some());
    assert_eq!(clean["exit_code"], 0);

    let dirty = mcp.call_tool("validate_workbook", json!({ "file": bad, "strict": true }));
    assert_eq!(dirty["valid"], false, "dirty validate: {dirty}");
    assert_ne!(dirty["exit_code"], 0);

    let err = mcp.call_tool_err("validate_workbook", json!({}));
    assert!(err.contains("missing required arg: file"), "got: {err}");
    assert_eq!(mcp.finish(), 0);
}

// ════════════════════════════════════════════════════════════════════════════
// list_pending / get_run_events
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn list_pending_empty_then_populated() {
    let tmp = tempfile::tempdir().unwrap();
    let file = write_wb(tmp.path(), "pause.md", PAUSE_WB);
    let mut mcp = McpClient::start();

    // Empty store: no error, and our run id is not present.
    let empty = mcp.call_tool("list_pending", json!({}));
    assert!(
        !empty.to_string().contains("pending-run-1"),
        "empty: {empty}"
    );

    // Pause a run, then it shows up.
    let run = mcp.call_tool(
        "run_workbook",
        json!({ "file": file, "run_id": "pending-run-1" }),
    );
    assert_eq!(run["status"], "input_required");
    let populated = mcp.call_tool("list_pending", json!({}));
    assert!(
        populated.to_string().contains("pending-run-1"),
        "populated: {populated}"
    );
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn get_run_events_after_completed_run() {
    let tmp = tempfile::tempdir().unwrap();
    let file = write_wb(tmp.path(), "pass.md", PASS_WB);
    let mut mcp = McpClient::start();
    let run = mcp.call_tool("run_workbook", json!({ "file": file, "run_id": "ev-run" }));
    assert_eq!(run["status"], "completed");

    let events = mcp.call_tool("get_run_events", json!({ "run_id": "ev-run" }));
    assert_eq!(events["run_id"], "ev-run");
    assert_eq!(events["status"], "complete");
    assert_eq!(events["terminal"]["event"], "run.complete");
    let evs = events["events"].as_array().unwrap();
    assert!(!evs.is_empty(), "expected step events: {events}");
    assert!(
        evs.iter().any(|e| e["event"] == "step.complete"),
        "no step.complete event: {events}"
    );
    let stdout: String = evs
        .iter()
        .filter_map(|e| e["stdout"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(stdout.contains("hello-from-wb"), "stdout: {stdout}");
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn get_run_events_unknown_id_errors() {
    let mut mcp = McpClient::start();
    let err = mcp.call_tool_err("get_run_events", json!({ "run_id": "nope-nope-123" }));
    assert!(err.contains("no run found"), "got: {err}");
    // Missing arg too.
    let err2 = mcp.call_tool_err("get_run_events", json!({}));
    assert!(err2.contains("missing required arg: run_id"), "got: {err2}");
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn get_run_events_terminal_failed_and_paused() {
    let tmp = tempfile::tempdir().unwrap();
    let fail = write_wb(tmp.path(), "fail.md", FAIL_WB);
    let pause = write_wb(tmp.path(), "pause.md", PAUSE_WB);
    let mut mcp = McpClient::start();

    // Failed run → terminal checkpoint.failed (status "failed").
    let run = mcp.call_tool(
        "run_workbook",
        json!({ "file": fail, "run_id": "tf", "bail": true }),
    );
    assert_eq!(run["status"], "failed");
    let ev = mcp.call_tool("get_run_events", json!({ "run_id": "tf" }));
    assert_eq!(ev["status"], "failed");
    assert_eq!(ev["terminal"]["event"], "checkpoint.failed");

    // Paused run → terminal workbook.paused with the elicitation echoed.
    let run2 = mcp.call_tool("run_workbook", json!({ "file": pause, "run_id": "tp" }));
    assert_eq!(run2["status"], "input_required");
    let ev2 = mcp.call_tool("get_run_events", json!({ "run_id": "tp" }));
    assert_eq!(ev2["status"], "paused");
    assert_eq!(ev2["terminal"]["event"], "workbook.paused");
    assert_eq!(ev2["terminal"]["elicitation"]["bind"][0], "token");
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn get_run_events_includes_skipped_steps() {
    // A conditionally-skipped block (skip_if truthy) records a step.skipped
    // entry in the checkpoint, exercising the skipped-events loop.
    let tmp = tempfile::tempdir().unwrap();
    let body = "# Skip\n\n```bash {skip_if=$ALWAYS}\necho should-not-run\n```\n\n```bash\necho ran-after\n```\n";
    let file = write_wb(tmp.path(), "skip.md", body);
    let mut mcp = McpClient::start();
    let run = mcp.call_tool(
        "run_workbook",
        json!({ "file": file, "run_id": "sk", "vars": { "ALWAYS": "1" } }),
    );
    assert_eq!(run["status"], "completed", "run: {run}");
    let ev = mcp.call_tool("get_run_events", json!({ "run_id": "sk" }));
    let evs = ev["events"].as_array().unwrap();
    assert!(
        evs.iter().any(|e| e["event"] == "step.skipped"),
        "expected a step.skipped event: {ev}"
    );
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn resume_goto_and_nonempty_rerun_argv_paths() {
    // Drive the goto_step and non-empty rerun_step argv branches. Unknown step
    // ids resolve to a usage/invalid result, which is fine — the point is to
    // exercise the argv-building code, not the navigation outcome.
    let tmp = tempfile::tempdir().unwrap();
    let pause = write_wb(tmp.path(), "pause.md", PAUSE_WB);
    let mut mcp = McpClient::start();

    let r1 = mcp.call_tool("run_workbook", json!({ "file": pause, "run_id": "g1" }));
    assert_eq!(r1["status"], "input_required");
    let resp = mcp.request(
        "tools/call",
        json!({ "name": "resume_workbook", "arguments": { "run_id": "g1", "goto_step": "no-such-id" } }),
    );
    assert!(resp["result"].get("content").is_some());

    // Re-author the (consumed) workbook with a fresh id and exercise non-empty
    // rerun_step.
    let pause2 = write_wb(tmp.path(), "pause2.md", PAUSE_WB);
    let r2 = mcp.call_tool("run_workbook", json!({ "file": pause2, "run_id": "g2" }));
    assert_eq!(r2["status"], "input_required");
    let resp2 = mcp.request(
        "tools/call",
        json!({ "name": "resume_workbook", "arguments": { "run_id": "g2", "rerun_step": "no-such-id" } }),
    );
    assert!(resp2["result"].get("content").is_some());
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn inspect_nonexistent_file_is_error() {
    // out.code != 0 with empty stdout → the inspect Err branch.
    let mut mcp = McpClient::start();
    let err = mcp.call_tool_err(
        "inspect_workbook",
        json!({ "file": "/no/such/path/definitely-missing.md" }),
    );
    assert!(err.contains("inspect failed"), "got: {err}");
    assert_eq!(mcp.finish(), 0);
}

// ════════════════════════════════════════════════════════════════════════════
// protocol-level errors
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn unknown_method_returns_method_not_found() {
    let mut mcp = McpClient::start();
    let resp = mcp.request("does/not/exist", json!({}));
    assert_eq!(resp["error"]["code"], -32601);
    assert!(resp["error"]["message"]
        .as_str()
        .unwrap()
        .contains("method not found"));
    assert!(resp.get("result").is_none());
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn tools_call_unknown_tool_name_is_error_result() {
    let mut mcp = McpClient::start();
    let resp = mcp.request(
        "tools/call",
        json!({ "name": "no_such_tool", "arguments": {} }),
    );
    // tools/call itself succeeds at the JSON-RPC layer; the unknown tool is
    // surfaced as an isError tool result.
    assert_eq!(resp["result"]["isError"], true);
    assert!(resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("unknown tool: no_such_tool"));
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn malformed_json_line_gets_parse_error_and_server_survives() {
    let mut mcp = McpClient::start();
    // A non-JSON line on stdin → parse error response with null id.
    mcp.write_line("this is not json {{{");
    let resp = mcp.read_response();
    assert_eq!(resp["error"]["code"], -32700);
    assert_eq!(resp["id"], Value::Null);
    assert!(resp["error"]["message"]
        .as_str()
        .unwrap()
        .contains("parse error"));

    // Server must still be alive and serving — a follow-up ping works.
    let pong = mcp.request("ping", json!({}));
    assert!(pong["result"].is_object());
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn notification_and_blank_lines_get_no_response() {
    let mut mcp = McpClient::start();
    // A notification (no id) yields no response; a blank line is skipped.
    mcp.notify("notifications/initialized");
    mcp.write_line("");
    mcp.notify("notifications/cancelled");
    // Next real request still answered in order (proves no stray responses).
    let resp = mcp.request("ping", json!({}));
    assert!(resp["result"].is_object());
    assert_eq!(resp["id"], 1);
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn response_shaped_message_without_method_is_ignored() {
    // A message with an id but no "method" (i.e. a JSON-RPC response sent to the
    // server) is not something our server role handles → it is silently skipped.
    let mut mcp = McpClient::start();
    mcp.write_line(&json!({ "jsonrpc": "2.0", "id": 99, "result": { "ok": true } }).to_string());
    // No response is produced; the next real request is still answered in order.
    let pong = mcp.request("ping", json!({}));
    assert!(pong["result"].is_object());
    assert_eq!(pong["id"], 1);
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn author_parent_dir_creation_failure_is_error() {
    // Make the would-be parent dir a regular file so create_dir_all fails.
    let tmp = tempfile::tempdir().unwrap();
    let blocker = tmp.path().join("blocker");
    std::fs::write(&blocker, "i am a file, not a dir").unwrap();
    let target = blocker.join("child.md"); // parent is a file → mkdir -p fails
    let mut mcp = McpClient::start();
    let err = mcp.call_tool_err(
        "author_workbook",
        json!({ "path": target.to_str().unwrap(), "content": PASS_WB }),
    );
    assert!(err.contains("create parent dir"), "got: {err}");
    assert_eq!(mcp.finish(), 0);
}

#[test]
fn missing_tool_name_falls_through_to_unknown_tool() {
    let mut mcp = McpClient::start();
    // tools/call with no `name` → empty name → unknown-tool error result.
    let resp = mcp.request("tools/call", json!({ "arguments": {} }));
    assert_eq!(resp["result"]["isError"], true);
    assert_eq!(mcp.finish(), 0);
}

// ════════════════════════════════════════════════════════════════════════════
// multi-request session + clean shutdown
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn full_session_author_run_events_then_clean_exit() {
    // One server, many requests in sequence: exercises the persistent stdin loop
    // across initialize → author → run → get_run_events, then a clean stdin-EOF
    // shutdown (exit 0).
    let tmp = tempfile::tempdir().unwrap();
    let wb_path = tmp.path().join("session.md");
    let wb_path_s = wb_path.to_str().unwrap().to_string();
    let mut mcp = McpClient::start();

    let init = mcp.request(
        "initialize",
        json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t", "version": "0" } }),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "wb");
    mcp.notify("notifications/initialized");

    let authored = mcp.call_tool(
        "author_workbook",
        json!({ "path": wb_path_s, "content": PASS_WB }),
    );
    assert_eq!(authored["path"], wb_path_s);

    let run = mcp.call_tool(
        "run_workbook",
        json!({ "file": wb_path_s, "run_id": "session-run" }),
    );
    assert_eq!(run["status"], "completed", "run: {run}");

    let events = mcp.call_tool("get_run_events", json!({ "run_id": "session-run" }));
    assert_eq!(events["terminal"]["event"], "run.complete");

    // Close stdin → server hits EOF and exits cleanly.
    assert_eq!(mcp.finish(), 0);
}
