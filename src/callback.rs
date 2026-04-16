use std::process::Command;

use chrono::Utc;
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::Sha256;

use crate::executor::BlockResult;

pub struct CallbackConfig {
    pub url: String,
    pub secret: Option<String>,
    pub stream_key: String,
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
        if self.is_redis() {
            self.send_redis(event, payload);
        } else {
            self.send_http(event, payload);
        }
    }

    fn send_http(&self, event: &str, payload: &str) {
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

        let mut conn = match client.get_connection_with_timeout(std::time::Duration::from_secs(5))
        {
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
        };
        assert!(!http_cb.is_redis());

        let redis_cb = CallbackConfig {
            url: "rediss://default:tok@my.upstash.io:6379".to_string(),
            secret: None,
            stream_key: "wb:events".to_string(),
        };
        assert!(redis_cb.is_redis());

        let redis_plain = CallbackConfig {
            url: "redis://default:tok@localhost:6379".to_string(),
            secret: None,
            stream_key: "wb:events".to_string(),
        };
        assert!(redis_plain.is_redis());
    }

    #[test]
    fn http_callback_not_redis() {
        let cb = CallbackConfig {
            url: "http://localhost:8080/hooks".to_string(),
            secret: Some("mysecret".to_string()),
            stream_key: "wb:events".to_string(),
        };
        assert!(!cb.is_redis());
    }
}
