//! `wb mcp` — a Model Context Protocol server over stdio.
//!
//! This is a *thin adapter*. It does not re-implement the run engine; it shells
//! out to the same `wb` binary (`current_exe`) for `run`/`inspect`/`validate`/
//! `resume`/`pending` and reads checkpoint + pending state in-process (read-only)
//! for `get_run_events`. That choice is deliberate:
//!
//! - `run_single` diverges via `std::process::exit` on pause (code 42) and on
//!   completion — calling it in-process would kill the long-lived server. A
//!   subprocess boundary turns that exit code into a value we can map.
//! - It honors the project guardrail "wb stays a CLI": the server *wraps* the
//!   binary; the core never becomes a daemon you must run.
//! - Zero new dependencies — just `serde_json`, already in the tree.
//!
//! ## Protocol
//!
//! JSON-RPC 2.0 over newline-delimited stdio (the MCP stdio transport). One JSON
//! object per line on stdin; one response object per line on stdout; logs go to
//! stderr. We implement the stable request subset: `initialize`,
//! `notifications/initialized`, `tools/list`, `tools/call`, `ping`.
//!
//! ## State mapping (the durable-execution → MCP bridge)
//!
//! - **Checkpoint + pending descriptor = the MCP Task store.** Every run is
//!   keyed by a `run_id` that is also the checkpoint id. `list_pending` is the
//!   list of tasks awaiting input; `get_run_events` is a task's timeline.
//! - **`pause_for_human` / `wait` → elicitation.** A paused run surfaces an
//!   `elicitation` object (message + the bound var(s) the resolver must supply)
//!   in the tool result. The client collects input and calls `resume_workbook`.
//!   We model this as data-in-the-result rather than a server-initiated
//!   `elicitation/create` round-trip because the producing subprocess has
//!   already exited — there is nothing to hold open mid-call.
//! - **Task status** rides on the child exit code: 0 → `completed`,
//!   42 → `input_required` (paused), 1 → `failed`, 7 → `timeout`, others → an
//!   error category.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};

use crate::checkpoint::{self, CheckpointStatus};
use crate::exit::WbExit;
use crate::exit_codes;
use crate::pending;

/// MCP protocol revision we author against. We echo the client's requested
/// version when it's one we recognize (lenient interop), else fall back here.
const PROTOCOL_VERSION: &str = "2025-06-18";
const KNOWN_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// Entry point for `wb mcp`. Runs the stdio server loop until stdin closes.
pub fn run() -> WbExit {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    let mut line = String::new();
    let mut handle = stdin.lock();

    loop {
        line.clear();
        match handle.read_line(&mut line) {
            Ok(0) => break, // EOF — client closed the pipe.
            Ok(_) => {}
            Err(e) => {
                crate::log_warn!("mcp: stdin read error: {}", e);
                break;
            }
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let msg: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                // Parse error with no recoverable id — emit a null-id error.
                write_message(
                    &mut out,
                    &error_response(Value::Null, -32700, &format!("parse error: {e}")),
                );
                continue;
            }
        };

        // A response (no "method") is unexpected for our server role; ignore.
        let Some(method) = msg.get("method").and_then(Value::as_str) else {
            continue;
        };
        let id = msg.get("id").cloned();
        let params = msg.get("params").cloned().unwrap_or(Value::Null);

        // Notifications carry no id and get no response.
        let is_notification = id.is_none();

        match method {
            "initialize" => {
                if let Some(id) = id {
                    write_message(&mut out, &success_response(id, initialize_result(&params)));
                }
            }
            "notifications/initialized" | "notifications/cancelled" => { /* no-op */ }
            "ping" => {
                if let Some(id) = id {
                    write_message(&mut out, &success_response(id, json!({})));
                }
            }
            "tools/list" => {
                if let Some(id) = id {
                    write_message(
                        &mut out,
                        &success_response(id, json!({ "tools": tool_definitions() })),
                    );
                }
            }
            "tools/call" => {
                if let Some(id) = id {
                    let resp = handle_tools_call(&params);
                    write_message(&mut out, &success_response(id, resp));
                }
            }
            other => {
                if !is_notification {
                    write_message(
                        &mut out,
                        &error_response(
                            id.unwrap_or(Value::Null),
                            -32601,
                            &format!("method not found: {other}"),
                        ),
                    );
                }
            }
        }
    }

    WbExit::Success
}

// ── JSON-RPC framing helpers ────────────────────────────────────────────────

fn write_message(out: &mut impl Write, msg: &Value) {
    // Newline-delimited JSON; messages must not contain embedded newlines, and
    // `serde_json::to_string` (compact) never emits them.
    let s = serde_json::to_string(msg).unwrap_or_else(|_| "{}".to_string());
    let _ = writeln!(out, "{s}");
    let _ = out.flush();
}

fn success_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn initialize_result(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version = match requested {
        Some(v) if KNOWN_PROTOCOL_VERSIONS.contains(&v) => v,
        _ => PROTOCOL_VERSION,
    };
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "wb", "version": env!("CARGO_PKG_VERSION") },
        "instructions":
            "wb runs markdown workbooks as code. author_workbook writes a .md file; \
             run_workbook executes it (returns a run_id = checkpoint id). A run can pause \
             on a `wait`/`pause_for_human` block — the result then carries status \
             \"input_required\" and an `elicitation` object; satisfy it with resume_workbook \
             (pass `value` or `signal`). list_pending shows runs awaiting input; \
             get_run_events replays a run's step timeline from its checkpoint."
    })
}

// ── Tool registry ───────────────────────────────────────────────────────────

fn tool_definitions() -> Value {
    json!([
        {
            "name": "author_workbook",
            "description": "Write a markdown workbook to disk so it can be run. Refuses to overwrite an existing file unless `overwrite` is true.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Destination .md path (created if missing)." },
                    "content": { "type": "string", "description": "Full markdown source (frontmatter + fenced code blocks)." },
                    "overwrite": { "type": "boolean", "description": "Overwrite an existing file. Default false." }
                },
                "required": ["path", "content"]
            }
        },
        {
            "name": "run_workbook",
            "description": "Execute a workbook. Returns a run_id (= checkpoint id) and a task status: \"completed\", \"input_required\" (paused on a wait/pause_for_human — see the `elicitation` field), \"failed\", or an error category. Resume a paused run with resume_workbook.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "Path to the workbook .md file." },
                    "run_id": { "type": "string", "description": "Stable id for this run (becomes the checkpoint id). Auto-generated if omitted." },
                    "vars": { "type": "object", "description": "Key/value env vars passed as --set KEY=VALUE.", "additionalProperties": { "type": "string" } },
                    "dir": { "type": "string", "description": "Working directory (-C)." },
                    "bail": { "type": "boolean", "description": "Stop on first failing block and checkpoint there. Default true." }
                },
                "required": ["file"]
            }
        },
        {
            "name": "resume_workbook",
            "description": "Resume a paused run (from a wait/pause_for_human). Supply the awaited input via `value` (single bound var) or `signal` (full JSON payload). Optional navigation: `rerun_step`, `goto_step`, or an `action` object.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "run_id": { "type": "string", "description": "The run/checkpoint id to resume." },
                    "value": { "type": "string", "description": "Shorthand: value for the single bound variable." },
                    "signal": { "type": "object", "description": "Full signal payload JSON (e.g. {\"otp_code\": \"123456\"})." },
                    "action": { "type": "object", "description": "Operator action object, e.g. {\"kind\":\"goto_step\",\"target\":\"open-inbox\"}." },
                    "rerun_step": { "type": "string", "description": "Re-run from this step id (empty string = re-run the currently paused step)." },
                    "goto_step": { "type": "string", "description": "Jump the cursor to this step id before resuming." }
                },
                "required": ["run_id"]
            }
        },
        {
            "name": "inspect_workbook",
            "description": "Show a workbook's structure (blocks, step ids, runtimes, sandbox config) without executing it. Returns wb inspect --json.",
            "inputSchema": {
                "type": "object",
                "properties": { "file": { "type": "string", "description": "Path to the workbook .md file." } },
                "required": ["file"]
            }
        },
        {
            "name": "validate_workbook",
            "description": "Static analysis of a workbook (frontmatter schema, durations, includes, fence attrs) with no execution. Returns wb validate --format json.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "Path to the workbook .md file or folder." },
                    "strict": { "type": "boolean", "description": "Treat warnings as failures. Default false." }
                },
                "required": ["file"]
            }
        },
        {
            "name": "list_pending",
            "description": "List runs paused awaiting input (the MCP \"tasks needing input\" view). Read-only — does not reap expired timeouts.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "get_run_events",
            "description": "Replay a run's step timeline (step.complete / step.skipped events + terminal status) reconstructed from its durable checkpoint. Use the run_id returned by run_workbook.",
            "inputSchema": {
                "type": "object",
                "properties": { "run_id": { "type": "string", "description": "The run/checkpoint id." } },
                "required": ["run_id"]
            }
        }
    ])
}

// ── tools/call dispatch ─────────────────────────────────────────────────────

fn handle_tools_call(params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    let result: Result<Value, String> = match name {
        "author_workbook" => tool_author(&args),
        "run_workbook" => tool_run(&args),
        "resume_workbook" => tool_resume(&args),
        "inspect_workbook" => tool_inspect(&args),
        "validate_workbook" => tool_validate(&args),
        "list_pending" => tool_list_pending(),
        "get_run_events" => tool_get_run_events(&args),
        other => Err(format!("unknown tool: {other}")),
    };

    match result {
        Ok(value) => tool_ok(value),
        Err(msg) => tool_err(&msg),
    }
}

/// Wrap a structured value as a successful MCP tool result: a JSON text block
/// (the canonical, widely-supported channel) plus `structuredContent` for
/// clients that read it.
fn tool_ok(value: Value) -> Value {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [ { "type": "text", "text": text } ],
        "structuredContent": value,
        "isError": false
    })
}

fn tool_err(message: &str) -> Value {
    json!({
        "content": [ { "type": "text", "text": message } ],
        "isError": true
    })
}

// ── Individual tools ────────────────────────────────────────────────────────

fn tool_author(args: &Value) -> Result<Value, String> {
    let path = str_arg(args, "path").ok_or("missing required arg: path")?;
    let content = str_arg(args, "content").ok_or("missing required arg: content")?;
    let overwrite = args
        .get("overwrite")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let p = std::path::Path::new(&path);
    if p.exists() && !overwrite {
        return Err(format!(
            "{path} already exists (pass overwrite=true to replace it)"
        ));
    }
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create parent dir: {e}"))?;
        }
    }
    std::fs::write(p, &content).map_err(|e| format!("write {path}: {e}"))?;
    Ok(json!({ "path": path, "bytes_written": content.len() }))
}

fn tool_run(args: &Value) -> Result<Value, String> {
    let file = str_arg(args, "file").ok_or("missing required arg: file")?;
    let run_id = str_arg(args, "run_id").unwrap_or_else(gen_run_id);
    let bail = args.get("bail").and_then(Value::as_bool).unwrap_or(true);

    let mut argv: Vec<String> = vec![
        "run".into(),
        file,
        "--json".into(),
        "-q".into(),
        "--checkpoint".into(),
        run_id.clone(),
    ];
    if bail {
        argv.push("--bail".into());
    }
    if let Some(dir) = str_arg(args, "dir") {
        argv.push("-C".into());
        argv.push(dir);
    }
    for (k, v) in object_str_map(args, "vars") {
        argv.push("--set".into());
        argv.push(format!("{k}={v}"));
    }

    let out = spawn_wb(&argv, None)?;
    Ok(run_result(&run_id, &out))
}

fn tool_resume(args: &Value) -> Result<Value, String> {
    let run_id = str_arg(args, "run_id").ok_or("missing required arg: run_id")?;

    let mut argv: Vec<String> = vec![
        "resume".into(),
        run_id.clone(),
        "--json".into(),
        "-q".into(),
    ];
    let mut stdin_payload: Option<String> = None;

    // Build the resume signal. `signal` (object) and `action` compose into one
    // JSON payload delivered on stdin; `value` is the single-bind shortcut.
    let signal = args.get("signal").filter(|v| !v.is_null()).cloned();
    let action = args.get("action").filter(|v| !v.is_null()).cloned();
    if signal.is_some() || action.is_some() {
        let mut payload = match signal {
            Some(Value::Object(m)) => Value::Object(m),
            Some(other) => return Err(format!("signal must be a JSON object, got {other}")),
            None => json!({}),
        };
        if let Some(action) = action {
            payload["action"] = action;
        }
        argv.push("--signal".into());
        argv.push("-".into());
        stdin_payload = Some(payload.to_string());
    } else if let Some(value) = str_arg(args, "value") {
        argv.push("--value".into());
        argv.push(value);
    }

    if let Some(rerun) = str_arg(args, "rerun_step") {
        argv.push("--rerun-step".into());
        if !rerun.is_empty() {
            argv.push(rerun);
        }
    } else if let Some(goto) = str_arg(args, "goto_step") {
        argv.push("--goto-step".into());
        argv.push(goto);
    }

    let out = spawn_wb(&argv, stdin_payload.as_deref())?;
    Ok(run_result(&run_id, &out))
}

fn tool_inspect(args: &Value) -> Result<Value, String> {
    let file = str_arg(args, "file").ok_or("missing required arg: file")?;
    let out = spawn_wb(&["inspect".into(), file, "--json".into()], None)?;
    if out.code != 0 && out.stdout.trim().is_empty() {
        return Err(format!(
            "inspect failed (exit {}): {}",
            out.code,
            out.stderr.trim()
        ));
    }
    Ok(parse_json_or_raw(&out.stdout))
}

fn tool_validate(args: &Value) -> Result<Value, String> {
    let file = str_arg(args, "file").ok_or("missing required arg: file")?;
    let strict = args.get("strict").and_then(Value::as_bool).unwrap_or(false);
    let mut argv: Vec<String> = vec!["validate".into(), file, "--format".into(), "json".into()];
    if strict {
        argv.push("--strict".into());
    }
    let out = spawn_wb(&argv, None)?;
    Ok(json!({
        "exit_code": out.code,
        "valid": out.code == exit_codes::EXIT_SUCCESS,
        "diagnostics": parse_json_or_raw(&out.stdout)
    }))
}

fn tool_list_pending() -> Result<Value, String> {
    let out = spawn_wb(
        &[
            "pending".into(),
            "--format".into(),
            "json".into(),
            "--no-reap".into(),
        ],
        None,
    )?;
    Ok(parse_json_or_raw(&out.stdout))
}

fn tool_get_run_events(args: &Value) -> Result<Value, String> {
    let run_id = str_arg(args, "run_id").ok_or("missing required arg: run_id")?;
    let ckpt = checkpoint::load(&run_id)
        .map_err(|e| format!("load checkpoint: {e}"))?
        .ok_or_else(|| format!("no run found for id '{run_id}'"))?;

    // Synthesize a step timeline from durable state: one event per saved result
    // (step.complete) and per terminal skip (step.skipped), ordered by block.
    #[derive(Clone)]
    struct Ev {
        block_index: usize,
        value: Value,
    }
    let mut evs: Vec<Ev> = Vec::new();

    for r in &ckpt.results {
        evs.push(Ev {
            block_index: r.block_index,
            value: json!({
                "event": "step.complete",
                "block_index": r.block_index,
                "step_id": r.step_id,
                "language": r.language,
                "exit_code": r.exit_code,
                "ok": r.exit_code == 0,
                "duration_ms": r.duration_ms,
                "line_number": r.line_number,
                "heading": r.heading,
                "stdout": r.stdout,
                "stderr": r.stderr,
            }),
        });
    }
    for s in &ckpt.skipped {
        evs.push(Ev {
            block_index: s.block_index,
            value: json!({
                "event": "step.skipped",
                "block_index": s.block_index,
                "step_id": s.step_id,
                "language": s.language,
                "line_number": s.line_number,
                "heading": s.heading,
                "kind": s.kind,
                "expression": s.expression,
                "reason": s.reason,
            }),
        });
    }
    evs.sort_by_key(|e| e.block_index);

    let mut events: Vec<Value> = Vec::with_capacity(evs.len() + 1);
    for (i, e) in evs.into_iter().enumerate() {
        let mut v = e.value;
        v["seq"] = json!(i);
        events.push(v);
    }

    // Terminal event derived from checkpoint status (+ pending descriptor when paused).
    let status_str = status_label(ckpt.status);
    let terminal = match ckpt.status {
        CheckpointStatus::Complete => json!({ "event": "run.complete", "status": "completed" }),
        CheckpointStatus::Failed => json!({ "event": "checkpoint.failed", "status": "failed" }),
        CheckpointStatus::Paused => {
            let elicit = pending::load(&run_id)
                .ok()
                .flatten()
                .map(|d| elicitation_from_pending(&d));
            json!({ "event": "workbook.paused", "status": "input_required", "elicitation": elicit })
        }
        CheckpointStatus::InProgress => json!({ "event": "run.in_progress", "status": "working" }),
    };

    Ok(json!({
        "run_id": run_id,
        "workbook": ckpt.workbook,
        "status": status_str,
        "next_block": ckpt.next_block,
        "next_step_id": ckpt.next_step_id,
        "total_blocks": ckpt.total_blocks,
        "started_at": ckpt.started_at,
        "updated_at": ckpt.updated_at,
        "outputs": ckpt.outputs,
        "events": events,
        "terminal": terminal
    }))
}

// ── Run-result mapping (exit code → task status) ────────────────────────────

/// Translate a finished `wb run`/`wb resume` subprocess into a task-shaped
/// result. The exit code is the source of truth for status; paused runs reload
/// the pending descriptor to expose the elicitation.
fn run_result(run_id: &str, out: &WbOutput) -> Value {
    let status = match out.code {
        exit_codes::EXIT_SUCCESS => "completed",
        exit_codes::EXIT_PAUSED => "input_required",
        exit_codes::EXIT_BLOCK_FAILED => "failed",
        exit_codes::EXIT_SIGNAL_TIMEOUT => "timeout",
        exit_codes::EXIT_USAGE => "usage_error",
        exit_codes::EXIT_WORKBOOK_INVALID => "invalid",
        exit_codes::EXIT_SANDBOX_UNAVAILABLE => "sandbox_unavailable",
        exit_codes::EXIT_CHECKPOINT_BUSY => "busy",
        _ => "error",
    };

    let mut result = json!({
        "run_id": run_id,
        "status": status,
        "exit_code": out.code,
        "stderr": out.stderr.trim_end(),
    });

    // On completion the child prints the run summary JSON to stdout; surface it.
    if out.code == exit_codes::EXIT_SUCCESS {
        let summary = parse_json_or_raw(&out.stdout);
        if !summary.is_null() {
            result["summary"] = summary;
        }
    }

    // Paused: reload the pending descriptor and expose the elicitation.
    if out.code == exit_codes::EXIT_PAUSED {
        if let Ok(Some(desc)) = pending::load(run_id) {
            result["elicitation"] = elicitation_from_pending(&desc);
            result["resume_url"] = json!(desc.resume_url);
        }
    }

    result
}

/// Build the MCP-flavored elicitation object from a pending descriptor. The
/// `requestedSchema` mirrors MCP elicitation: one string property per bound var
/// the resolver must supply (or a single free-form `signal` when no bind).
fn elicitation_from_pending(desc: &pending::PendingDescriptor) -> Value {
    let binds: Vec<String> = match &desc.bind {
        Some(crate::parser::BindSpec::Single(s)) => vec![s.clone()],
        Some(crate::parser::BindSpec::Multiple(v)) => v.clone(),
        None => Vec::new(),
    };

    let mut properties = serde_json::Map::new();
    if binds.is_empty() {
        properties.insert(
            "signal".into(),
            json!({ "type": "string", "description": "Free-form signal value to resume with." }),
        );
    } else {
        for b in &binds {
            properties.insert(
                b.clone(),
                json!({ "type": "string", "description": format!("Value for `{b}`.") }),
            );
        }
    }

    let message = desc.message.clone().unwrap_or_else(|| match &desc.kind {
        Some(k) => format!("Run paused awaiting a `{k}` signal."),
        None => "Run paused awaiting input.".to_string(),
    });

    json!({
        "message": message,
        "kind": desc.kind,
        "bind": binds,
        "actions": desc.actions,
        "context_url": desc.context_url,
        "timeout_at": desc.timeout_at,
        "requestedSchema": { "type": "object", "properties": properties, "required": binds }
    })
}

fn status_label(s: CheckpointStatus) -> &'static str {
    match s {
        CheckpointStatus::InProgress => "in_progress",
        CheckpointStatus::Complete => "complete",
        CheckpointStatus::Failed => "failed",
        CheckpointStatus::Paused => "paused",
    }
}

// ── Subprocess plumbing ─────────────────────────────────────────────────────

struct WbOutput {
    code: i32,
    stdout: String,
    stderr: String,
}

/// Run the `wb` binary itself (`current_exe`) with `argv`, optionally feeding
/// `stdin_data`. Inherits the server's environment so checkpoint dirs, secrets,
/// and `WB_*` settings carry through to the child.
fn spawn_wb(argv: &[String], stdin_data: Option<&str>) -> Result<WbOutput, String> {
    let exe = std::env::current_exe().map_err(|e| format!("locate wb binary: {e}"))?;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(argv);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.stdin(if stdin_data.is_some() {
        std::process::Stdio::piped()
    } else {
        std::process::Stdio::null()
    });

    let mut child = cmd.spawn().map_err(|e| format!("spawn wb: {e}"))?;
    if let Some(data) = stdin_data {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(data.as_bytes())
                .map_err(|e| format!("write child stdin: {e}"))?;
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("wait for wb: {e}"))?;
    Ok(WbOutput {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

// ── Small helpers ───────────────────────────────────────────────────────────

fn str_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(|s| s.to_string())
}

fn object_str_map(args: &Value, key: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    if let Some(obj) = args.get(key).and_then(Value::as_object) {
        for (k, v) in obj {
            if let Some(s) = v.as_str() {
                map.insert(k.clone(), s.to_string());
            } else {
                map.insert(k.clone(), v.to_string());
            }
        }
    }
    map
}

/// Parse `s` as JSON; if it isn't valid JSON, return it as a JSON string so the
/// tool result is always well-formed. Empty → JSON null.
fn parse_json_or_raw(s: &str) -> Value {
    let t = s.trim();
    if t.is_empty() {
        return Value::Null;
    }
    serde_json::from_str(t).unwrap_or_else(|_| Value::String(t.to_string()))
}

static RUN_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique-ish run id when the caller doesn't supply one. Combines
/// pid, a wall-clock nanos stamp, and a process-local counter so concurrent
/// tool calls don't collide.
fn gen_run_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = RUN_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("mcp-{}-{}-{}", std::process::id(), nanos, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_echoes_known_protocol_version() {
        let res = initialize_result(&json!({ "protocolVersion": "2025-03-26" }));
        assert_eq!(res["protocolVersion"], "2025-03-26");
        assert_eq!(res["serverInfo"]["name"], "wb");
        assert!(res["capabilities"]["tools"].is_object());
    }

    #[test]
    fn initialize_falls_back_on_unknown_version() {
        let res = initialize_result(&json!({ "protocolVersion": "1999-01-01" }));
        assert_eq!(res["protocolVersion"], PROTOCOL_VERSION);
    }

    #[test]
    fn tools_list_has_required_tools() {
        let tools = tool_definitions();
        let names: Vec<&str> = tools
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        for expected in [
            "author_workbook",
            "run_workbook",
            "resume_workbook",
            "inspect_workbook",
            "validate_workbook",
            "list_pending",
            "get_run_events",
        ] {
            assert!(names.contains(&expected), "missing tool {expected}");
        }
        // Every tool must carry an object inputSchema.
        for t in tools.as_array().unwrap() {
            assert_eq!(
                t["inputSchema"]["type"], "object",
                "tool {} bad schema",
                t["name"]
            );
        }
    }

    #[test]
    fn run_result_maps_exit_codes_to_status() {
        let cases = [
            (0, "completed"),
            (42, "input_required"),
            (1, "failed"),
            (7, "timeout"),
            (2, "usage_error"),
            (3, "invalid"),
            (99, "error"),
        ];
        for (code, want) in cases {
            let out = WbOutput {
                code,
                stdout: String::new(),
                stderr: String::new(),
            };
            let r = run_result("rid", &out);
            assert_eq!(r["status"], want, "exit {code}");
            assert_eq!(r["run_id"], "rid");
            assert_eq!(r["exit_code"], code);
        }
    }

    #[test]
    fn run_result_attaches_summary_on_success() {
        let out = WbOutput {
            code: 0,
            stdout: r#"{"passed":2,"failed":0}"#.to_string(),
            stderr: String::new(),
        };
        let r = run_result("rid", &out);
        assert_eq!(r["summary"]["passed"], 2);
    }

    #[test]
    fn elicitation_single_bind_builds_schema() {
        use crate::parser::BindSpec;
        let mut desc = bare_descriptor();
        desc.bind = Some(BindSpec::Single("otp_code".into()));
        desc.kind = Some("email".into());
        let e = elicitation_from_pending(&desc);
        assert_eq!(e["bind"][0], "otp_code");
        assert!(e["requestedSchema"]["properties"]["otp_code"].is_object());
        assert_eq!(e["requestedSchema"]["required"][0], "otp_code");
    }

    #[test]
    fn elicitation_no_bind_uses_free_form_signal() {
        let desc = bare_descriptor();
        let e = elicitation_from_pending(&desc);
        assert!(e["requestedSchema"]["properties"]["signal"].is_object());
    }

    #[test]
    fn author_refuses_overwrite_without_flag() {
        let dir = std::env::temp_dir().join(format!("wb-mcp-author-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("wb.md");
        std::fs::write(&path, "x").unwrap();
        let args = json!({ "path": path.to_str().unwrap(), "content": "y" });
        let err = tool_author(&args).unwrap_err();
        assert!(err.contains("already exists"), "got: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_json_or_raw_handles_all_shapes() {
        assert!(parse_json_or_raw("").is_null());
        assert_eq!(parse_json_or_raw(r#"{"a":1}"#)["a"], 1);
        assert_eq!(
            parse_json_or_raw("not json"),
            Value::String("not json".into())
        );
    }

    fn bare_descriptor() -> pending::PendingDescriptor {
        pending::PendingDescriptor {
            checkpoint: "c".into(),
            checkpoint_id: "c".into(),
            workbook: "w.md".into(),
            next_block: 1,
            next_step_id: None,
            line_number: 1,
            section_index: 1,
            kind: None,
            match_: None,
            bind: None,
            created_at: "now".into(),
            timeout_at: None,
            on_timeout: None,
            sidecar_state: None,
            resume_url: None,
            verb_index: None,
            message: None,
            context_url: None,
            resume_on: None,
            timeout: None,
            actions: Vec::new(),
            callback_url: None,
            callback_secret: None,
        }
    }
}
