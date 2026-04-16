use std::process::Command;

use chrono::Utc;
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::Sha256;

use crate::executor::BlockResult;

pub struct CallbackConfig {
    pub url: String,
    pub secret: Option<String>,
}

impl CallbackConfig {
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
        let payload = json!({
            "event": "step.complete",
            "checkpoint_id": checkpoint_id,
            "workbook": workbook,
            "block": {
                "index": result.block_index,
                "language": &result.language,
                "heading": heading,
                "line_number": line_number,
                "exit_code": result.exit_code,
                "duration_ms": result.duration.as_millis() as u64,
            },
            "progress": {
                "completed": completed,
                "total": total,
            },
            "timestamp": Utc::now().to_rfc3339(),
        });
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
            "checkpoint_id": checkpoint_id,
            "workbook": workbook,
            "failed_block": {
                "index": result.block_index,
                "language": &result.language,
                "heading": heading,
                "line_number": line_number,
                "exit_code": result.exit_code,
                "stderr": &result.stderr,
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
        let event_header = format!("X-WB-Event: {}", event);
        let sig_header;

        let mut args = vec![
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
            &event_header,
        ];

        if let Some(ref secret) = self.secret {
            sig_header = format!(
                "X-WB-Signature: sha256={}",
                sign(payload.as_bytes(), secret.as_bytes())
            );
            args.push("-H");
            args.push(&sig_header);
        }

        args.push("-d");
        args.push(payload);
        args.push(&self.url);

        match Command::new("curl").args(&args).output() {
            Ok(output) => {
                let code = String::from_utf8_lossy(&output.stdout);
                let code = code.trim();
                if !code.starts_with('2') {
                    eprintln!("warning: callback {} returned HTTP {}", event, code);
                }
            }
            Err(e) => {
                eprintln!("warning: callback failed: {}", e);
            }
        }
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
