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

/// Info captured when a slice pauses for a human-in-the-loop action
/// (MFA, OTP, etc.). Returned up to the executor + main so `wb` can write a
/// pending descriptor and exit 42.
#[derive(Debug, Clone)]
pub struct PauseInfo {
    pub sidecar_state: Option<serde_yaml::Value>,
    pub reason: Option<String>,
    pub resume_url: Option<String>,
    pub verb_index: Option<usize>,
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
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        // Best-effort shutdown. Send a `shutdown` frame so the sidecar can close
        // browser contexts cleanly; then kill if it ignores us.
        let _ = writeln!(self.stdin, "{}", json!({ "type": "shutdown" }));
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
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
            for line in reader.lines().flatten() {
                if tx_out.send(line).is_err() {
                    break;
                }
            }
        });

        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().flatten() {
                crate::output::print_stderr_dim(&format!("[sidecar] {}", line));
            }
        });

        let mut sc = Sidecar {
            child,
            stdin,
            events: rx,
            binary,
        };

        sc.handshake()?;
        Ok(sc)
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
            "line_number": spec.line_number,
            "section_index": spec.section_index,
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
                                let reason = v
                                    .get("reason")
                                    .and_then(|x| x.as_str())
                                    .unwrap_or("paused");
                                let url = v
                                    .get("resume_url")
                                    .and_then(|x| x.as_str())
                                    .unwrap_or("");
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
                            let err = v.get("error").and_then(|x| x.as_str()).unwrap_or("slice failed");
                            stderr_buf.push_str(err);
                            stderr_buf.push('\n');
                            return SliceOutcome {
                                exit_code: 1,
                                stdout: stdout_buf,
                                stderr: stderr_buf,
                                pause: None,
                            };
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
                if k != "type" {
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
    PauseInfo {
        sidecar_state,
        reason,
        resume_url,
        verb_index,
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
        "browser runtime binary not found. Install `{name}` on $PATH, or set \
WB_BROWSER_RUNTIME=/path/to/{name}. See runtimes/browser/ in the wb repo for the \
reference implementation.",
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
        assert_eq!(info.resume_url.as_deref(), Some("https://browserbase/live/xyz"));
        assert_eq!(info.verb_index, Some(7));
        assert!(info.sidecar_state.is_some());
    }
}
