//! Sidecar runtime bridge — line-framed JSON over stdio.
//!
//! The `browser` runtime doesn't run as a subprocess-per-block like bash/python.
//! Instead, `wb` spawns a long-lived sidecar (e.g. `wb-browser-runtime`, a Node
//! process that owns Playwright/Stagehand/Browserbase) at the first `browser`
//! block, and forwards each slice as a single JSON command. The sidecar replies
//! with a stream of JSON events — one per verb, mid-slice `slice.recovered` /
//! `slice.paused` notifications, plus a terminal `slice.complete` /
//! `slice.failed` / `slice.paused`.
//!
//! This module is intentionally dumb: `wb` doesn't interpret verbs. Whatever
//! the sidecar returns in `stdout` / `stderr` fields gets handed back up to the
//! executor's `BlockResult`. Slice-paused state is an opaque YAML value `wb`
//! persists in the pending descriptor and hands back on resume.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::callback::CallbackConfig;
use crate::parser::BrowserSliceSpec;

/// Binary name we look for on $PATH when WB_BROWSER_RUNTIME is unset.
const DEFAULT_SIDECAR_BINARY: &str = "wb-browser-runtime";
/// Max time to wait for the `ready` handshake reply from a freshly spawned sidecar.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
/// Max time to wait for the next event while a slice is running.
const SLICE_EVENT_TIMEOUT: Duration = Duration::from_secs(300);
/// Max time to wait for the sidecar to exit cleanly after `shutdown` before
/// we fall back to SIGKILL. Covers `flushRecording` (rrweb drain + ffmpeg
/// finalize + per-kind upload) which has its own ~30s per-upload budget.
/// Overridable via `WB_SIDECAR_SHUTDOWN_TIMEOUT_SECS` for test harnesses.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(45);

/// Info captured when a slice pauses for a human-in-the-loop action.
/// Generalized from the original MFA-only shape to carry the full
/// `pause_for_human` payload so the run page can render operator-facing
/// affordances (message, deep-link, action buttons) from a single event.
/// Returned up to the executor + main so `wb` can write a pending
/// descriptor and exit 42.
#[derive(Debug, Clone, Default)]
pub struct PauseInfo {
    pub sidecar_state: Option<serde_yaml::Value>,
    pub reason: Option<String>,
    pub resume_url: Option<String>,
    pub verb_index: Option<usize>,
    /// Operator-facing message (e.g. "Drop this month's receipts, then
    /// resume"). Rendered as the primary prompt on the run page.
    pub message: Option<String>,
    /// Deep-link the operator clicks to take the required off-band action
    /// (MFA page, Drive folder, approval console, ...). Optional — some
    /// pauses are pure wait-for-ack notifications.
    pub context_url: Option<String>,
    /// One of "operator_click" | "poll" | "timeout". The run page picks
    /// its UI + auto-resume behavior from this.
    pub resume_on: Option<String>,
    /// Duration string forwarded from the verb args (e.g. "1h", "5m"). wb
    /// doesn't parse it here — the descriptor's `timeout_at` is the
    /// authoritative wall-clock deadline; this field is for display.
    pub timeout: Option<String>,
    /// Operator button set. Empty vec (or missing) is rendered as a single
    /// "Resume" button; a non-empty list becomes a branching choice whose
    /// selected value ends up in `$WB_ARTIFACTS_DIR/pause_result.json`
    /// via the resume path.
    pub actions: Vec<serde_json::Value>,
}

/// Result of running one `browser` slice through the sidecar.
///
/// `pause` is `Some` when the sidecar emitted `slice.paused` — the slice did
/// NOT complete; the executor is expected to persist pending state and exit.
pub struct SliceOutcome {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub pause: Option<PauseInfo>,
}

/// Context needed for mid-slice callbacks (step.recovered / step.paused).
/// Not stored; borrowed for the duration of `run_slice`.
pub struct SliceCallbackContext<'a> {
    pub cb: Option<&'a CallbackConfig>,
    pub workbook: &'a str,
    pub checkpoint_id: Option<&'a str>,
    pub block_index: usize,
    pub heading: Option<&'a str>,
    pub line_number: usize,
    pub completed: usize,
    pub total: usize,
    /// Snapshot of active include frames (outermost → innermost) at the time
    /// this browser slice executes. Forwarded on `step.paused` / `step.resumed`
    /// / `step.recovered` events so consumers can correlate mid-slice
    /// lifecycle with the operator-visible step timeline.
    pub include_chain: &'a [crate::parser::IncludeFrame],
}

/// Extra payload sent to the sidecar to resume a previously-paused slice.
/// `state` is opaque (came from a prior `slice.paused` event) and `signal`
/// carries whatever the external resolver gave `wb resume` (e.g. OTP code).
#[derive(Debug, Clone, Default)]
pub struct RestoreArgs {
    pub state: Option<serde_yaml::Value>,
    pub signal: Option<serde_json::Value>,
}

/// A running sidecar process + line-framed JSON channel.
pub struct Sidecar {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    events: Receiver<String>,
    binary: String,
    suspended: bool,
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        if self.suspended {
            return;
        }
        // Best-effort shutdown. Send a `shutdown` frame so the sidecar can
        // flush recordings + close browser contexts cleanly, then give it a
        // bounded window to exit on its own before falling back to SIGKILL.
        let _ = writeln!(self.stdin, "{}", json!({ "type": "shutdown" }));
        let _ = self.stdin.flush();

        let deadline = Instant::now() + shutdown_timeout();
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() >= deadline => break,
                Ok(None) => thread::sleep(Duration::from_millis(100)),
                Err(_) => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Read `WB_SIDECAR_SHUTDOWN_TIMEOUT_SECS` if set to a non-negative integer,
/// otherwise fall back to `SHUTDOWN_TIMEOUT`. A value of `0` disables the
/// wait entirely (legacy SIGKILL-immediately behavior) for tests that need
/// fast teardown and aren't exercising the recording flush path.
fn shutdown_timeout() -> Duration {
    match std::env::var("WB_SIDECAR_SHUTDOWN_TIMEOUT_SECS") {
        Ok(v) => match v.trim().parse::<u64>() {
            Ok(n) => Duration::from_secs(n),
            Err(_) => SHUTDOWN_TIMEOUT,
        },
        Err(_) => SHUTDOWN_TIMEOUT,
    }
}

impl Sidecar {
    /// Spawn the sidecar and complete the hello/ready handshake.
    pub fn spawn(env: &HashMap<String, String>, working_dir: &str) -> Result<Self, String> {
        let binary = resolve_binary()?;

        let mut cmd = Command::new(&binary);
        cmd.current_dir(working_dir);
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("failed to spawn sidecar {}: {}", binary, e))?;

        let stdin = BufWriter::new(child.stdin.take().unwrap());
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let (tx, rx) = mpsc::channel::<String>();

        let tx_out = tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if tx_out.send(line).is_err() {
                    break;
                }
            }
        });

        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                crate::output::print_stderr_dim(&format!("[sidecar] {}", line));
            }
        });

        let mut sc = Sidecar {
            child,
            stdin,
            events: rx,
            binary,
            suspended: false,
        };

        sc.handshake()?;
        Ok(sc)
    }

    /// Ask the sidecar to suspend instead of shutting down.
    ///
    /// Used for browser-slice pauses: the remote browser must remain alive so
    /// the operator can complete the handoff in the live inspector and a later
    /// `wb resume` can reconnect to the same vendor session. Unlike Drop's
    /// shutdown path, the sidecar must not close the browser or release the
    /// vendor session.
    pub fn suspend(&mut self) -> Result<(), String> {
        writeln!(self.stdin, "{}", json!({ "type": "suspend" }))
            .map_err(|e| format!("sidecar suspend write failed: {}", e))?;
        self.stdin
            .flush()
            .map_err(|e| format!("sidecar suspend flush failed: {}", e))?;

        let deadline = Instant::now() + shutdown_timeout();
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => {
                    self.suspended = true;
                    return Ok(());
                }
                Ok(None) if Instant::now() >= deadline => {
                    return Err(format!(
                        "sidecar {} did not suspend within {}s",
                        self.binary,
                        shutdown_timeout().as_secs()
                    ));
                }
                Ok(None) => thread::sleep(Duration::from_millis(100)),
                Err(e) => return Err(format!("sidecar suspend wait failed: {}", e)),
            }
        }
    }

    fn handshake(&mut self) -> Result<(), String> {
        let hello = json!({
            "type": "hello",
            "wb_version": env!("CARGO_PKG_VERSION"),
            "protocol": "wb-sidecar/1",
        });
        writeln!(self.stdin, "{}", hello)
            .map_err(|e| format!("sidecar handshake write failed: {}", e))?;
        self.stdin
            .flush()
            .map_err(|e| format!("sidecar handshake flush failed: {}", e))?;

        let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(format!(
                    "sidecar {} did not reply `ready` within {}s",
                    self.binary,
                    HANDSHAKE_TIMEOUT.as_secs()
                ));
            }
            match self.events.recv_timeout(remaining) {
                Ok(line) => {
                    let v: Value = match serde_json::from_str(&line) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    if v.get("type").and_then(|t| t.as_str()) == Some("ready") {
                        return Ok(());
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(format!(
                        "sidecar {} did not reply `ready` within {}s",
                        self.binary,
                        HANDSHAKE_TIMEOUT.as_secs()
                    ));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(format!("sidecar {} exited during handshake", self.binary));
                }
            }
        }
    }

    /// Run one slice and collect events until slice.{complete,paused,failed}.
    ///
    /// Fires mid-slice callbacks (`step.recovered`, `step.paused`) via `ctx.cb`
    /// as the sidecar reports them.
    pub fn run_slice(
        &mut self,
        spec: &BrowserSliceSpec,
        quiet: bool,
        ctx: &SliceCallbackContext,
        restore: Option<&RestoreArgs>,
    ) -> SliceOutcome {
        let mut command = json!({
            "type": "slice",
            "session": spec.session,
            "on_pause": spec.on_pause,
            "profile": spec.profile,
            "line_number": spec.line_number,
            "section_index": spec.section_index,
            "block_index": ctx.block_index,
            "verbs": spec.verbs,
        });

        if let Some(r) = restore {
            // Sidecar sees a single message with a `restore` field — cleaner
            // than a separate protocol state machine.
            let mut restore_obj = serde_json::Map::new();
            if let Some(state) = r.state.as_ref() {
                if let Ok(v) = serde_json::to_value(state) {
                    restore_obj.insert("state".to_string(), v);
                }
            }
            if let Some(signal) = r.signal.as_ref() {
                restore_obj.insert("signal".to_string(), signal.clone());
            }
            if !restore_obj.is_empty() {
                command["restore"] = Value::Object(restore_obj);
            }
        }

        if let Err(e) = writeln!(self.stdin, "{}", command) {
            return SliceOutcome {
                exit_code: 1,
                stdout: String::new(),
                stderr: format!("sidecar write failed: {}", e),
                pause: None,
            };
        }
        if let Err(e) = self.stdin.flush() {
            return SliceOutcome {
                exit_code: 1,
                stdout: String::new(),
                stderr: format!("sidecar flush failed: {}", e),
                pause: None,
            };
        }

        let mut stdout_buf = String::new();
        let mut stderr_buf = String::new();
        loop {
            match self.events.recv_timeout(SLICE_EVENT_TIMEOUT) {
                Ok(line) => {
                    let v: Value = match serde_json::from_str(&line) {
                        Ok(v) => v,
                        Err(_) => {
                            stdout_buf.push_str(&line);
                            stdout_buf.push('\n');
                            continue;
                        }
                    };
                    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match ty {
                        "verb.complete" => {
                            let verb = v.get("verb").and_then(|x| x.as_str()).unwrap_or("verb");
                            let summary = v.get("summary").and_then(|x| x.as_str()).unwrap_or("ok");
                            let line_str = format!("✓ {} — {}", verb, summary);
                            if !quiet {
                                println!("{}", line_str);
                            }
                            stdout_buf.push_str(&line_str);
                            stdout_buf.push('\n');
                        }
                        "verb.failed" => {
                            let verb = v.get("verb").and_then(|x| x.as_str()).unwrap_or("verb");
                            let err = v.get("error").and_then(|x| x.as_str()).unwrap_or("failed");
                            let line_str = format!("✗ {} — {}", verb, err);
                            if !quiet {
                                crate::output::print_stderr_dim(&line_str);
                            }
                            stderr_buf.push_str(&line_str);
                            stderr_buf.push('\n');
                        }
                        "slice.recovered" => {
                            // AI recovery fixed a selector — fire step.recovered callback
                            // so consumers can patch the runbook source.
                            if !quiet {
                                let verb = v.get("verb").and_then(|x| x.as_str()).unwrap_or("verb");
                                let note = v
                                    .get("recovered_selector")
                                    .and_then(|x| x.as_str())
                                    .unwrap_or("recovered");
                                crate::output::print_stderr_dim(&format!(
                                    "  ↻ recovered {} — {}",
                                    verb, note
                                ));
                            }
                            fire_lifecycle(ctx, "step.recovered", &v);
                        }
                        "slice.paused" => {
                            // Slice is paused for a human action. Fire step.paused
                            // callback + return a Paused outcome so wb writes the
                            // pending descriptor and exits EXIT_PAUSED.
                            if !quiet {
                                let reason =
                                    v.get("reason").and_then(|x| x.as_str()).unwrap_or("paused");
                                let url =
                                    v.get("resume_url").and_then(|x| x.as_str()).unwrap_or("");
                                crate::output::print_stderr_dim(&format!(
                                    "  ⏸ {} — {}",
                                    reason, url
                                ));
                            }
                            fire_lifecycle(ctx, "step.paused", &v);
                            return SliceOutcome {
                                exit_code: 0,
                                stdout: stdout_buf,
                                stderr: stderr_buf,
                                pause: Some(extract_pause_info(&v)),
                            };
                        }
                        "slice.complete" => {
                            return SliceOutcome {
                                exit_code: 0,
                                stdout: stdout_buf,
                                stderr: stderr_buf,
                                pause: None,
                            };
                        }
                        "slice.failed" => {
                            let err = v
                                .get("error")
                                .and_then(|x| x.as_str())
                                .unwrap_or("slice failed");
                            stderr_buf.push_str(err);
                            stderr_buf.push('\n');
                            return SliceOutcome {
                                exit_code: 1,
                                stdout: stdout_buf,
                                stderr: stderr_buf,
                                pause: None,
                            };
                        }
                        ty if ty.starts_with("slice.") => {
                            // Generic lifecycle passthrough — any non-terminal
                            // slice.* event flows as a callback so new event
                            // types ship without a wb release.
                            let event_name = derive_lifecycle_event_name(ty);
                            if !quiet {
                                crate::output::print_stderr_dim(&format!(
                                    "  · {} → {}",
                                    ty, event_name
                                ));
                            }
                            fire_lifecycle(ctx, &event_name, &v);
                        }
                        _ => {
                            stdout_buf.push_str(&line);
                            stdout_buf.push('\n');
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    stderr_buf.push_str(&format!(
                        "sidecar timed out after {}s waiting for slice event\n",
                        SLICE_EVENT_TIMEOUT.as_secs()
                    ));
                    return SliceOutcome {
                        exit_code: -1,
                        stdout: stdout_buf,
                        stderr: stderr_buf,
                        pause: None,
                    };
                }
                Err(RecvTimeoutError::Disconnected) => {
                    stderr_buf.push_str("sidecar exited unexpectedly\n");
                    return SliceOutcome {
                        exit_code: -1,
                        stdout: stdout_buf,
                        stderr: stderr_buf,
                        pause: None,
                    };
                }
            }
        }
    }
}

fn fire_lifecycle(ctx: &SliceCallbackContext, event: &str, sidecar_msg: &Value) {
    let Some(cb) = ctx.cb else { return };
    // Pull everything the sidecar emitted (except the `type` field) into `extra`.
    let extra = match sidecar_msg {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if k != "type" && k != "sidecar_state" {
                    out.insert(k.clone(), v.clone());
                }
            }
            Value::Object(out)
        }
        _ => Value::Null,
    };
    cb.step_lifecycle(
        event,
        ctx.workbook,
        ctx.checkpoint_id,
        ctx.block_index,
        "browser",
        ctx.heading,
        ctx.line_number,
        ctx.completed,
        ctx.total,
        extra,
        ctx.include_chain,
    );
}

fn extract_pause_info(msg: &Value) -> PauseInfo {
    let sidecar_state = msg
        .get("sidecar_state")
        .and_then(|v| serde_yaml::to_value(v).ok());
    let reason = msg
        .get("reason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let resume_url = msg
        .get("resume_url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let verb_index = msg
        .get("verb_index")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize);
    let message = msg
        .get("message")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let context_url = msg
        .get("context_url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let resume_on = msg
        .get("resume_on")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let timeout = msg
        .get("timeout")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    // `actions` is forwarded verbatim. Anything non-array (or missing) is
    // treated as "no custom actions" — the run page will render a single
    // default Resume button. We keep the list opaque so sidecar-side schema
    // evolution (e.g. adding icon/hint fields) doesn't need a wb release.
    let actions = msg
        .get("actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    PauseInfo {
        sidecar_state,
        reason,
        resume_url,
        verb_index,
        message,
        context_url,
        resume_on,
        timeout,
        actions,
    }
}

/// Map a sidecar `slice.<suffix>` event name to the callback event we publish.
///
/// Convention:
///   `slice.session_*` → `session.*`   (run-scoped lifecycle, e.g. live URL)
///   `slice.<other>`   → `step.<other>` (block-scoped lifecycle)
///
/// Falls back to the original name if the prefix isn't `slice.`, so callers
/// don't need to pre-check.
fn derive_lifecycle_event_name(slice_event: &str) -> String {
    let Some(suffix) = slice_event.strip_prefix("slice.") else {
        return slice_event.to_string();
    };
    if let Some(rest) = suffix.strip_prefix("session_") {
        format!("session.{}", rest)
    } else {
        format!("step.{}", suffix)
    }
}

/// Resolve the sidecar binary path.
///
/// 1. `WB_BROWSER_RUNTIME` env var — absolute path or bare command name.
/// 2. `wb-browser-runtime` on `$PATH`.
fn resolve_binary() -> Result<String, String> {
    if let Ok(p) = std::env::var("WB_BROWSER_RUNTIME") {
        if !p.trim().is_empty() {
            return Ok(p);
        }
    }
    if which_on_path(DEFAULT_SIDECAR_BINARY).is_some() {
        return Ok(DEFAULT_SIDECAR_BINARY.to_string());
    }
    Err(format!(
        "browser runtime binary not found. Install with `npm i -g {name}` (requires \
Node 18+), or set WB_BROWSER_RUNTIME=/path/to/{name} to point at a custom build.",
        name = DEFAULT_SIDECAR_BINARY
    ))
}

fn which_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // Combined test: cargo runs tests on multiple threads and env vars are
    // process-global, so we verify both branches in a single sequential body.
    #[test]
    fn resolve_binary_env_and_missing() {
        let saved_env = std::env::var_os("WB_BROWSER_RUNTIME");
        let saved_path = std::env::var_os("PATH");

        // 1. Env override wins over $PATH.
        std::env::set_var("WB_BROWSER_RUNTIME", "/tmp/custom-sidecar");
        let r = resolve_binary().unwrap();
        assert_eq!(r, "/tmp/custom-sidecar");

        // 2. No env, $PATH without the binary → error.
        std::env::remove_var("WB_BROWSER_RUNTIME");
        std::env::set_var("PATH", "/nonexistent-wb-test-path");
        let r2 = resolve_binary();
        assert!(r2.is_err(), "expected err, got {:?}", r2);

        // restore
        match saved_path {
            Some(v) => std::env::set_var("PATH", v),
            None => std::env::remove_var("PATH"),
        }
        match saved_env {
            Some(v) => std::env::set_var("WB_BROWSER_RUNTIME", v),
            None => std::env::remove_var("WB_BROWSER_RUNTIME"),
        }
    }

    #[test]
    fn extract_pause_info_parses_all_fields() {
        let msg = json!({
            "type": "slice.paused",
            "reason": "airbase_totp",
            "resume_url": "https://browserbase/live/xyz",
            "verb_index": 7,
            "sidecar_state": { "step": "awaiting_totp", "nav": "abc" }
        });
        let info = extract_pause_info(&msg);
        assert_eq!(info.reason.as_deref(), Some("airbase_totp"));
        assert_eq!(
            info.resume_url.as_deref(),
            Some("https://browserbase/live/xyz")
        );
        assert_eq!(info.verb_index, Some(7));
        assert!(info.sidecar_state.is_some());
    }

    #[test]
    fn extract_pause_info_parses_pause_for_human_fields() {
        let msg = json!({
            "type": "slice.paused",
            "reason": "pause_for_human",
            "message": "Drop this month's receipts in the folder below",
            "context_url": "https://drive.google.com/drive/folders/abc",
            "resume_on": "operator_click",
            "timeout": "1h",
            "actions": [
                { "label": "Approved", "value": "approved" },
                { "label": "Denied", "value": "denied" }
            ],
            "verb_index": 0,
            "sidecar_state": { "verb_index": 0 }
        });
        let info = extract_pause_info(&msg);
        assert_eq!(info.reason.as_deref(), Some("pause_for_human"));
        assert_eq!(
            info.message.as_deref(),
            Some("Drop this month's receipts in the folder below")
        );
        assert_eq!(
            info.context_url.as_deref(),
            Some("https://drive.google.com/drive/folders/abc")
        );
        assert_eq!(info.resume_on.as_deref(), Some("operator_click"));
        assert_eq!(info.timeout.as_deref(), Some("1h"));
        assert_eq!(info.actions.len(), 2);
        assert_eq!(info.actions[0]["label"], "Approved");
        assert_eq!(info.actions[1]["value"], "denied");
    }

    #[test]
    fn extract_pause_info_defaults_missing_pause_for_human_fields() {
        // Existing wait_for_mfa-style pauses that don't carry the new fields
        // must still parse cleanly — PauseInfo fields default to None / [].
        let msg = json!({
            "type": "slice.paused",
            "reason": "legacy_mfa",
            "verb_index": 3
        });
        let info = extract_pause_info(&msg);
        assert_eq!(info.reason.as_deref(), Some("legacy_mfa"));
        assert!(info.message.is_none());
        assert!(info.context_url.is_none());
        assert!(info.resume_on.is_none());
        assert!(info.timeout.is_none());
        assert!(info.actions.is_empty());
    }

    #[test]
    fn derive_event_name_session_prefix_routes_to_session_namespace() {
        assert_eq!(
            derive_lifecycle_event_name("slice.session_started"),
            "session.started"
        );
        assert_eq!(
            derive_lifecycle_event_name("slice.session_closed"),
            "session.closed"
        );
    }

    #[test]
    fn derive_event_name_other_slice_routes_to_step_namespace() {
        assert_eq!(
            derive_lifecycle_event_name("slice.network_idle"),
            "step.network_idle"
        );
        assert_eq!(
            derive_lifecycle_event_name("slice.screenshot_taken"),
            "step.screenshot_taken"
        );
    }

    #[test]
    fn derive_event_name_passes_through_non_slice_prefix() {
        // Defensive: shouldn't be reachable from the dispatcher, but the
        // helper is total — non-slice inputs round-trip unchanged.
        assert_eq!(
            derive_lifecycle_event_name("verb.complete"),
            "verb.complete"
        );
    }
}
