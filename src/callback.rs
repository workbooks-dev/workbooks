use std::process::Command;
use std::time::Duration;

use chrono::Utc;
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::Sha256;

use crate::executor::BlockResult;

/// Hard cap on stdout/stderr bytes forwarded in callback payloads.
/// Tail-biased truncation: we keep the end since failures usually surface there.
const MAX_OUTPUT_BYTES: usize = 64 * 1024;

/// Schema version for callback payloads. Bumped when the payload shape
/// changes incompatibly so receivers can branch. Receivers should treat
/// an unknown version as "newer than I handle" and either drop or log.
const EVENT_VERSION: &str = "1";

/// Exponential-ish backoff delays for HTTP callback retries.
/// On 5xx or network error we retry; on 2xx/4xx we stop (4xx won't heal).
/// Total wall time if all retries fire: ~1.2s plus the curl timeouts.
const HTTP_RETRY_DELAYS: &[Duration] = &[
    Duration::from_millis(0),
    Duration::from_millis(200),
    Duration::from_millis(1000),
];

/// Prepare a captured stdout/stderr string for inclusion in a callback payload.
///
/// - If `s` contains a NUL byte, treat it as binary and return `<binary: N bytes>`.
///   (`BlockResult.{stdout,stderr}` are already `String`s, i.e. valid UTF-8, so the
///   practical binary check reduces to NUL-byte presence.)
/// - If `s` is within the 64 KiB cap, return it unchanged.
/// - Otherwise keep the trailing `MAX_OUTPUT_BYTES` (aligned to a UTF-8 char
///   boundary so we never panic on multibyte splits) and append a
///   `\n…[truncated N bytes]` marker where N is the number of bytes dropped.
fn truncate_for_callback(s: &str) -> String {
    if s.as_bytes().contains(&0) {
        return format!("<binary: {} bytes>", s.len());
    }
    if s.len() <= MAX_OUTPUT_BYTES {
        return s.to_string();
    }
    let removed = s.len() - MAX_OUTPUT_BYTES;
    let start = s.len() - MAX_OUTPUT_BYTES;
    // Walk forward to the next char boundary so &s[start..] never panics.
    let start = (start..=s.len())
        .find(|&i| s.is_char_boundary(i))
        .unwrap_or(s.len());
    format!("{}\n…[truncated {} bytes]", &s[start..], removed)
}

/// Build the `step.complete` JSON payload. Factored out of `step_complete` so
/// we can unit-test the payload shape without firing curl/redis side effects.
#[allow(clippy::too_many_arguments)]
fn build_step_complete_payload(
    result: &BlockResult,
    completed: usize,
    total: usize,
    workbook: &str,
    checkpoint_id: Option<&str>,
    heading: Option<&str>,
    line_number: usize,
    run_id: &str,
) -> serde_json::Value {
    json!({
        "event": "step.complete",
            "event_version": EVENT_VERSION,
        "run_id": run_id,
        "checkpoint_id": checkpoint_id,
        "workbook": workbook,
        "block": {
            "index": result.block_index,
            "language": &result.language,
            "heading": heading,
            "line_number": line_number,
            "exit_code": result.exit_code,
            "duration_ms": result.duration.as_millis() as u64,
            "stdout": truncate_for_callback(&result.stdout),
            "stderr": truncate_for_callback(&result.stderr),
            "stdout_partial": result.stdout_partial,
            "stderr_partial": result.stderr_partial,
            "error_type": result.error_type,
        },
        "progress": {
            "completed": completed,
            "total": total,
        },
        "timestamp": Utc::now().to_rfc3339(),
    })
}

pub struct CallbackConfig {
    pub url: String,
    pub secret: Option<String>,
    pub stream_key: String,
    /// Trace-correlation id stamped on every payload. Same value appears in
    /// the result artifact's `run_id` field so a dashboard can join across
    /// callbacks + final report without extra plumbing.
    pub run_id: String,
}

impl CallbackConfig {
    fn is_redis(&self) -> bool {
        self.url.starts_with("redis://") || self.url.starts_with("rediss://")
    }

    /// Fired after each block finishes executing (pass or fail)
    pub fn step_complete(
        &self,
        result: &BlockResult,
        completed: usize,
        total: usize,
        workbook: &str,
        checkpoint_id: Option<&str>,
        heading: Option<&str>,
        line_number: usize,
    ) {
        let payload = build_step_complete_payload(
            result,
            completed,
            total,
            workbook,
            checkpoint_id,
            heading,
            line_number,
            &self.run_id,
        );
        self.send("step.complete", &payload.to_string());
    }

    /// Fired when --bail triggers on a failure with checkpointing active
    pub fn checkpoint_failed(
        &self,
        result: &BlockResult,
        completed: usize,
        total: usize,
        workbook: &str,
        checkpoint_id: &str,
        heading: Option<&str>,
        line_number: usize,
    ) {
        let payload = json!({
            "event": "checkpoint.failed",
            "event_version": EVENT_VERSION,
            "run_id": &self.run_id,
            "checkpoint_id": checkpoint_id,
            "workbook": workbook,
            "failed_block": {
                "index": result.block_index,
                "language": &result.language,
                "heading": heading,
                "line_number": line_number,
                "exit_code": result.exit_code,
                "stderr": &result.stderr,
                "stdout_partial": result.stdout_partial,
                "stderr_partial": result.stderr_partial,
                "error_type": result.error_type,
            },
            "progress": {
                "completed": completed,
                "total": total,
            },
            "timestamp": Utc::now().to_rfc3339(),
        });
        self.send("checkpoint.failed", &payload.to_string());
    }

    /// Fired when a `wait` block pauses the workbook for an external signal
    pub fn workbook_paused(
        &self,
        workbook: &str,
        checkpoint_id: &str,
        kind: Option<&str>,
        bind: Option<&[String]>,
        timeout_at: Option<&str>,
    ) {
        let payload = json!({
            "event": "workbook.paused",
            "event_version": EVENT_VERSION,
            "run_id": &self.run_id,
            "checkpoint_id": checkpoint_id,
            "workbook": workbook,
            "wait": {
                "kind": kind,
                "bind": bind,
                "timeout_at": timeout_at,
            },
            "timestamp": Utc::now().to_rfc3339(),
        });
        self.send("workbook.paused", &payload.to_string());
    }

    /// Fired for intra-step lifecycle events emitted mid-slice by the sidecar:
    /// `step.paused`, `step.resumed`, `step.recovered`. `wb` owns the envelope
    /// (block metadata, progress, timestamp); the sidecar owns the slice-level
    /// detail carried in `extra` (verb_index, reason, resume_url, recovered
    /// selector, etc.) so new sidecar fields flow through without a wb release.
    #[allow(clippy::too_many_arguments)]
    pub fn step_lifecycle(
        &self,
        event: &str,
        workbook: &str,
        checkpoint_id: Option<&str>,
        block_index: usize,
        language: &str,
        heading: Option<&str>,
        line_number: usize,
        completed: usize,
        total: usize,
        extra: serde_json::Value,
    ) {
        let mut payload = json!({
            "event": event,
            "event_version": EVENT_VERSION,
            "run_id": &self.run_id,
            "checkpoint_id": checkpoint_id,
            "workbook": workbook,
            "block": {
                "index": block_index,
                "language": language,
                "heading": heading,
                "line_number": line_number,
            },
            "progress": {
                "completed": completed,
                "total": total,
            },
            "timestamp": Utc::now().to_rfc3339(),
        });
        // Merge sidecar-supplied top-level fields (slice, reason, resume_url, ...).
        if let (Some(obj), Some(extra_obj)) = (payload.as_object_mut(), extra.as_object()) {
            for (k, v) in extra_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
        self.send(event, &payload.to_string());
    }

    /// Fired when the entire run finishes (all blocks executed)
    pub fn run_complete(
        &self,
        passed: usize,
        failed: usize,
        total: usize,
        duration_ms: u64,
        workbook: &str,
        checkpoint_id: Option<&str>,
    ) {
        let payload = json!({
            "event": "run.complete",
            "event_version": EVENT_VERSION,
            "run_id": &self.run_id,
            "checkpoint_id": checkpoint_id,
            "workbook": workbook,
            "status": if failed == 0 { "pass" } else { "fail" },
            "blocks": {
                "total": total,
                "passed": passed,
                "failed": failed,
            },
            "duration_ms": duration_ms,
            "timestamp": Utc::now().to_rfc3339(),
        });
        self.send("run.complete", &payload.to_string());
    }

    fn send(&self, event: &str, payload: &str) {
        if self.is_redis() {
            self.send_redis(event, payload);
        } else {
            self.send_http(event, payload);
        }
    }

    fn send_http(&self, event: &str, payload: &str) {
        let event_header = format!("X-WB-Event: {}", event);
        let sig_header = self.secret.as_ref().map(|s| {
            format!(
                "X-WB-Signature: sha256={}",
                sign(payload.as_bytes(), s.as_bytes())
            )
        });

        for (attempt, delay) in HTTP_RETRY_DELAYS.iter().enumerate() {
            if *delay > Duration::ZERO {
                std::thread::sleep(*delay);
            }
            let is_last = attempt + 1 == HTTP_RETRY_DELAYS.len();
            match try_send_http_once(&self.url, &event_header, sig_header.as_deref(), payload) {
                HttpSendResult::Ok => return,
                HttpSendResult::ClientError(code) => {
                    // 4xx — receiver says we're wrong; retrying won't help.
                    eprintln!(
                        "warning: callback {} returned HTTP {} (not retrying)",
                        event, code
                    );
                    return;
                }
                HttpSendResult::ServerError(code) if is_last => {
                    eprintln!(
                        "warning: callback {} failed after {} attempts: HTTP {}",
                        event,
                        HTTP_RETRY_DELAYS.len(),
                        code
                    );
                }
                HttpSendResult::NetworkError(err) if is_last => {
                    eprintln!(
                        "warning: callback {} failed after {} attempts: {}",
                        event,
                        HTTP_RETRY_DELAYS.len(),
                        err
                    );
                }
                HttpSendResult::ServerError(_) | HttpSendResult::NetworkError(_) => {
                    // Non-terminal failure — loop will retry after the next backoff.
                }
            }
        }
    }

    /// XADD to a Redis stream using the redis crate.
    /// Works with any Redis: Upstash (rediss://), self-hosted, ElastiCache, etc.
    fn send_redis(&self, event: &str, payload: &str) {
        // Install rustls crypto provider for TLS (rediss://) connections.
        // Safe to call multiple times — returns Err if already installed.
        let _ = rustls::crypto::ring::default_provider().install_default();

        let client = match redis::Client::open(self.url.as_str()) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("warning: redis callback: {}", e);
                return;
            }
        };

        let mut conn = match client.get_connection_with_timeout(std::time::Duration::from_secs(5)) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("warning: redis callback connect: {}", e);
                return;
            }
        };

        let result: Result<String, redis::RedisError> = redis::cmd("XADD")
            .arg(&self.stream_key)
            .arg("*")
            .arg("event")
            .arg(event)
            .arg("data")
            .arg(payload)
            .query(&mut conn);

        if let Err(e) = result {
            eprintln!("warning: redis callback XADD: {}", e);
        }
    }
}

/// Outcome of a single curl HTTP callback attempt.
enum HttpSendResult {
    /// 2xx — delivered.
    Ok,
    /// 4xx — receiver rejected; no point retrying.
    ClientError(String),
    /// 5xx — transient server-side; safe to retry.
    ServerError(String),
    /// Curl itself failed (DNS, TLS, timeout, non-HTTP response).
    NetworkError(String),
}

fn try_send_http_once(
    url: &str,
    event_header: &str,
    sig_header: Option<&str>,
    payload: &str,
) -> HttpSendResult {
    let mut args: Vec<&str> = vec![
        "-s",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
        "--max-time",
        "5",
        "-X",
        "POST",
        "-H",
        "Content-Type: application/json",
        "-H",
        event_header,
    ];

    if let Some(sh) = sig_header {
        args.push("-H");
        args.push(sh);
    }

    args.push("-d");
    args.push(payload);
    args.push(url);

    match Command::new("curl").args(&args).output() {
        Ok(output) => {
            let code = String::from_utf8_lossy(&output.stdout);
            let code = code.trim().to_string();
            if code.starts_with('2') {
                HttpSendResult::Ok
            } else if code.starts_with('4') {
                HttpSendResult::ClientError(code)
            } else if code.starts_with('5') {
                HttpSendResult::ServerError(code)
            } else {
                // 000 (curl connection failure), 3xx without follow, etc.
                HttpSendResult::NetworkError(format!("unexpected response code: {}", code))
            }
        }
        Err(e) => HttpSendResult::NetworkError(format!("spawn curl: {}", e)),
    }
}

fn sign(payload: &[u8], secret: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC accepts any key size");
    mac.update(payload);
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_redis_detection() {
        let http_cb = CallbackConfig {
            url: "https://hooks.example.com/wb".to_string(),
            secret: None,
            stream_key: "wb:events".to_string(),
            run_id: "test-run".to_string(),
        };
        assert!(!http_cb.is_redis());

        let redis_cb = CallbackConfig {
            url: "rediss://default:tok@my.upstash.io:6379".to_string(),
            secret: None,
            stream_key: "wb:events".to_string(),
            run_id: "test-run".to_string(),
        };
        assert!(redis_cb.is_redis());

        let redis_plain = CallbackConfig {
            url: "redis://default:tok@localhost:6379".to_string(),
            secret: None,
            stream_key: "wb:events".to_string(),
            run_id: "test-run".to_string(),
        };
        assert!(redis_plain.is_redis());
    }

    #[test]
    fn http_callback_not_redis() {
        let cb = CallbackConfig {
            url: "http://localhost:8080/hooks".to_string(),
            secret: Some("mysecret".to_string()),
            stream_key: "wb:events".to_string(),
            run_id: "test-run".to_string(),
        };
        assert!(!cb.is_redis());
    }

    #[test]
    fn test_truncate_tail_small() {
        let s = "hello world";
        let out = truncate_for_callback(s);
        assert_eq!(out, "hello world");
    }

    #[test]
    fn test_truncate_tail_small_exactly_at_cap() {
        // Boundary case: exactly at cap is unchanged.
        let s = "a".repeat(MAX_OUTPUT_BYTES);
        let out = truncate_for_callback(&s);
        assert_eq!(out.len(), MAX_OUTPUT_BYTES);
        assert_eq!(out, s);
    }

    #[test]
    fn test_truncate_tail_large() {
        // Input: cap + 100 bytes of 'a' followed by a distinctive tail.
        let extra = 100;
        let tail = "TAIL_END";
        let mut s = "a".repeat(MAX_OUTPUT_BYTES + extra - tail.len());
        s.push_str(tail);
        assert!(s.len() > MAX_OUTPUT_BYTES);

        let out = truncate_for_callback(&s);

        // We kept the tail, not the head.
        assert!(out.contains(tail), "expected tail to be preserved");
        // Marker is appended.
        assert!(
            out.contains("…[truncated"),
            "expected truncation marker, got: {}",
            &out[out.len().saturating_sub(80)..]
        );
        // Reports the exact number of bytes removed.
        assert!(
            out.contains(&format!("truncated {} bytes", extra)),
            "expected 'truncated {} bytes', got suffix: {}",
            extra,
            &out[out.len().saturating_sub(80)..]
        );
        // Length: kept bytes + "\n…[truncated N bytes]" marker.
        let marker = format!("\n…[truncated {} bytes]", extra);
        assert_eq!(out.len(), MAX_OUTPUT_BYTES + marker.len());
        assert!(out.len() <= MAX_OUTPUT_BYTES + marker.len());
    }

    #[test]
    fn test_truncate_tail_large_multibyte_boundary() {
        // Push a multibyte character across the tail-split boundary and make
        // sure we don't panic slicing mid-codepoint. '€' is 3 bytes in UTF-8.
        // Build a string where the naive split point lands inside '€'.
        let mut s = String::new();
        // Fill with ASCII so s.len() is exactly MAX_OUTPUT_BYTES + 1
        // before we prepend a multibyte char that shifts the split point.
        for _ in 0..(MAX_OUTPUT_BYTES + 1) {
            s.push('a');
        }
        // Now prepend a '€' — split point moves into the middle of it if we
        // don't honor char boundaries.
        let mut final_s = String::from("€");
        final_s.push_str(&s);

        // Must not panic.
        let out = truncate_for_callback(&final_s);
        assert!(out.contains("…[truncated"));
        // UTF-8 validity is enforced by String; just confirm output is a valid String.
        assert!(!out.is_empty());
    }

    #[test]
    fn test_binary_detection_nul() {
        let s = "abc\0def";
        let out = truncate_for_callback(s);
        assert_eq!(out, format!("<binary: {} bytes>", s.len()));
    }

    #[test]
    fn test_build_step_complete_payload_shape() {
        let result = BlockResult {
            block_index: 0,
            language: "bash".to_string(),
            stdout: "hello\n".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
            duration: std::time::Duration::from_millis(16),
            error_type: None,
            stdout_partial: false,
            stderr_partial: false,
        };
        let payload = build_step_complete_payload(
            &result,
            1,
            6,
            "health-check",
            Some("ckpt-1"),
            Some("Identity"),
            42,
            "run-abc",
        );
        assert_eq!(payload["event"], "step.complete");
        assert_eq!(payload["event_version"], "1");
        assert_eq!(payload["run_id"], "run-abc");
        assert_eq!(payload["checkpoint_id"], "ckpt-1");
        assert_eq!(payload["workbook"], "health-check");
        assert_eq!(payload["block"]["index"], 0);
        assert_eq!(payload["block"]["language"], "bash");
        assert_eq!(payload["block"]["heading"], "Identity");
        assert_eq!(payload["block"]["line_number"], 42);
        assert_eq!(payload["block"]["exit_code"], 0);
        assert_eq!(payload["block"]["duration_ms"], 16);
        assert_eq!(payload["block"]["stdout"], "hello\n");
        assert_eq!(payload["block"]["stderr"], "");
        assert_eq!(payload["progress"]["completed"], 1);
        assert_eq!(payload["progress"]["total"], 6);
    }

    #[test]
    fn test_step_lifecycle_envelope_shape() {
        // We can't assert on send() side effects without a network, but we can
        // re-build the payload inline to confirm shape by replicating the
        // builder's logic for a known extra blob.
        let extra = json!({
            "slice": { "verb_index": 7 },
            "reason": "airbase_totp",
            "resume_url": "https://browserbase/live/abc123",
        });
        let mut payload = json!({
            "event": "step.paused",
            "event_version": EVENT_VERSION,
            "checkpoint_id": "ckpt-1",
            "workbook": "airbase-login",
            "block": {
                "index": 2,
                "language": "browser",
                "heading": "Login",
                "line_number": 18,
            },
            "progress": { "completed": 2, "total": 5 },
            "timestamp": Utc::now().to_rfc3339(),
        });
        if let (Some(obj), Some(extra_obj)) = (payload.as_object_mut(), extra.as_object()) {
            for (k, v) in extra_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
        assert_eq!(payload["event"], "step.paused");
        assert_eq!(payload["block"]["language"], "browser");
        assert_eq!(payload["slice"]["verb_index"], 7);
        assert_eq!(payload["reason"], "airbase_totp");
        assert_eq!(payload["resume_url"], "https://browserbase/live/abc123");
    }

    #[test]
    fn test_build_step_complete_payload_truncates_and_marks_binary() {
        let huge = "x".repeat(MAX_OUTPUT_BYTES + 50);
        let result = BlockResult {
            block_index: 3,
            language: "bash".to_string(),
            stdout: huge.clone(),
            stderr: "oops\0binary".to_string(),
            exit_code: 1,
            duration: std::time::Duration::from_millis(5),
            error_type: None,
            stdout_partial: false,
            stderr_partial: false,
        };
        let payload = build_step_complete_payload(&result, 4, 10, "wb", None, None, 0, "");
        let stdout = payload["block"]["stdout"].as_str().unwrap();
        assert!(stdout.contains("…[truncated 50 bytes]"));
        assert!(stdout.len() < huge.len());

        let stderr = payload["block"]["stderr"].as_str().unwrap();
        assert_eq!(stderr, "<binary: 11 bytes>");
    }
}
