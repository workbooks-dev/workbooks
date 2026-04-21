//! Per-cell artifact capture and upload.
//!
//! `wb` exposes `$WB_ARTIFACTS_DIR` to every cell. Anything a cell writes
//! there is treated as a runbook artifact: persisted locally so the next
//! cell can read it, and optionally POSTed to `$WB_ARTIFACTS_UPLOAD_URL`
//! (template — see below) for first-party storage.
//!
//! Upload is fire-and-forget-ish: each file gets a short curl call after
//! the cell that produced it completes. Failures are logged and skipped —
//! artifact capture never fails a run.
//!
//! Env vars:
//! - `WB_ARTIFACTS_DIR` — set by wb, read by cells. Default location is
//!   `~/.wb/runs/<run_id>/artifacts/` when a run id is available (via
//!   `WB_RECORDING_RUN_ID` or `TRIGGER_RUN_ID`), otherwise
//!   `$TMPDIR/wb-artifacts-<uuid>/`.
//! - `WB_ARTIFACTS_UPLOAD_URL` — optional; template supports `{run_id}`
//!   and `{filename}` placeholders. When unset, artifacts stay local-only.
//! - `WB_RECORDING_UPLOAD_SECRET` — reused for Bearer auth; required when
//!   upload URL is set.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

/// Environment-variable name cells read to discover the artifacts dir.
pub const ENV_DIR: &str = "WB_ARTIFACTS_DIR";
pub const ENV_UPLOAD_URL: &str = "WB_ARTIFACTS_UPLOAD_URL";
pub const ENV_UPLOAD_SECRET: &str = "WB_RECORDING_UPLOAD_SECRET";
pub const ENV_RUN_ID: &str = "WB_RECORDING_RUN_ID";
pub const ENV_TRIGGER_RUN_ID: &str = "TRIGGER_RUN_ID";

pub struct Artifacts {
    dir: PathBuf,
    run_id: String,
    upload_url: Option<String>,
    upload_secret: Option<String>,
    /// Files we've already uploaded, keyed by (path, mtime-as-nanos).
    /// Rewriting a file produces a new mtime and triggers a re-upload.
    seen: HashMap<PathBuf, u128>,
}

impl Artifacts {
    /// Resolve the artifacts dir from `env`, create it, and return a handle.
    /// Also mutates `env` so the path is propagated to every spawned cell.
    ///
    /// Resolution order: frontmatter `env:` block → process env (so an
    /// orchestrator like rav can `export WB_ARTIFACTS_DIR=...` before
    /// `wb run`) → default under `~/.wb/runs/<run_id>/artifacts`.
    pub fn init(env: &mut HashMap<String, String>) -> Self {
        let run_id = resolve_run_id(env);

        let dir = env
            .get(ENV_DIR)
            .filter(|s| !s.is_empty())
            .cloned()
            .or_else(|| std::env::var(ENV_DIR).ok().filter(|s| !s.is_empty()))
            .map(PathBuf::from)
            .unwrap_or_else(|| default_dir(&run_id));

        if let Err(e) = fs::create_dir_all(&dir) {
            // Non-fatal: the dir may not be writable, but cells can still
            // run. Fall back to a unique tmp dir so subsequent uploads
            // don't try to read from a missing path.
            eprintln!(
                "warning: could not create WB_ARTIFACTS_DIR at {}: {} — falling back to tmp",
                dir.display(),
                e
            );
            let fallback = std::env::temp_dir().join(format!("wb-artifacts-{}", short_rand()));
            let _ = fs::create_dir_all(&fallback);
            env.insert(ENV_DIR.to_string(), fallback.to_string_lossy().into_owned());
            return Artifacts {
                dir: fallback,
                run_id,
                upload_url: upload_url(env),
                upload_secret: upload_secret(env),
                seen: HashMap::new(),
            };
        }

        env.insert(ENV_DIR.to_string(), dir.to_string_lossy().into_owned());

        Artifacts {
            dir,
            run_id,
            upload_url: upload_url(env),
            upload_secret: upload_secret(env),
            seen: HashMap::new(),
        }
    }

    /// Scan the artifacts dir for files that are new (or have a newer mtime
    /// than last time we saw them) and upload each one. Called after every
    /// cell completes. Safe to call when uploads are disabled — it still
    /// records mtimes so subsequent rewrites are detected.
    pub fn sync(&mut self) {
        let entries = match fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let mtime = match entry.metadata().and_then(|m| m.modified()) {
                Ok(t) => system_time_to_nanos(t),
                Err(_) => 0,
            };

            let previous = self.seen.get(&path).copied();
            if previous == Some(mtime) {
                continue;
            }
            self.seen.insert(path.clone(), mtime);

            // Upload is optional — if there's no target, we just tracked
            // the mtime for debouncing and move on.
            if self.upload_url.is_some() && self.upload_secret.is_some() {
                self.upload_one(&path);
            }
        }
    }

    fn upload_one(&self, path: &Path) {
        let Some(url_template) = self.upload_url.as_deref() else {
            return;
        };
        let Some(secret) = self.upload_secret.as_deref() else {
            return;
        };
        let filename = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => return,
        };

        let url = url_template
            .replace("{run_id}", &url_encode(&self.run_id))
            .replace("{filename}", &url_encode(&filename));

        let auth = format!("Authorization: Bearer {}", secret);
        let content_type = guess_content_type(&filename);
        let content_type_header = format!("Content-Type: {}", content_type);
        let run_id_header = format!("X-WB-Run-Id: {}", self.run_id);
        let filename_header = format!("X-WB-Artifact-Filename: {}", filename);

        let path_arg = path.to_string_lossy().into_owned();
        let data_arg = format!("@{}", path_arg);
        let args = vec![
            "-s",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "--max-time",
            "30",
            "-X",
            "POST",
            "-H",
            &auth,
            "-H",
            &content_type_header,
            "-H",
            &run_id_header,
            "-H",
            &filename_header,
            "--data-binary",
            &data_arg,
            &url,
        ];

        match Command::new("curl").args(&args).output() {
            Ok(out) => {
                let code = String::from_utf8_lossy(&out.stdout);
                let code = code.trim();
                if !code.starts_with('2') {
                    eprintln!(
                        "warning: artifact upload {} returned HTTP {}",
                        filename, code
                    );
                }
            }
            Err(e) => {
                eprintln!("warning: artifact upload {}: {}", filename, e);
            }
        }
    }
}

pub fn resolve_run_id(env: &HashMap<String, String>) -> String {
    if let Some(v) = env.get(ENV_RUN_ID).filter(|s| !s.is_empty()) {
        return v.clone();
    }
    if let Some(v) = env.get(ENV_TRIGGER_RUN_ID).filter(|s| !s.is_empty()) {
        return v.clone();
    }
    if let Ok(v) = std::env::var(ENV_RUN_ID) {
        if !v.is_empty() {
            return v;
        }
    }
    if let Ok(v) = std::env::var(ENV_TRIGGER_RUN_ID) {
        if !v.is_empty() {
            return v;
        }
    }
    format!("wb-{}", short_rand())
}

fn upload_url(env: &HashMap<String, String>) -> Option<String> {
    env.get(ENV_UPLOAD_URL)
        .filter(|s| !s.is_empty())
        .cloned()
        .or_else(|| std::env::var(ENV_UPLOAD_URL).ok().filter(|s| !s.is_empty()))
}

fn upload_secret(env: &HashMap<String, String>) -> Option<String> {
    env.get(ENV_UPLOAD_SECRET)
        .filter(|s| !s.is_empty())
        .cloned()
        .or_else(|| std::env::var(ENV_UPLOAD_SECRET).ok().filter(|s| !s.is_empty()))
}

fn default_dir(run_id: &str) -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".wb")
            .join("runs")
            .join(sanitize(run_id))
            .join("artifacts");
    }
    std::env::temp_dir()
        .join(format!("wb-artifacts-{}", sanitize(run_id)))
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn short_rand() -> String {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}", nanos & 0xffff_ffff)
}

fn system_time_to_nanos(t: SystemTime) -> u128 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn guess_content_type(filename: &str) -> &'static str {
    let lower = filename.to_ascii_lowercase();
    if lower.ends_with(".json") {
        "application/json"
    } else if lower.ends_with(".yaml") || lower.ends_with(".yml") {
        "application/yaml"
    } else if lower.ends_with(".txt") || lower.ends_with(".log") {
        "text/plain"
    } else if lower.ends_with(".csv") {
        "text/csv"
    } else if lower.ends_with(".md") {
        "text/markdown"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".html") {
        "text/html"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_dir_and_sets_env() {
        let mut env = HashMap::new();
        env.insert(ENV_RUN_ID.to_string(), "test-run-1".to_string());

        let tmp = std::env::temp_dir().join(format!("wb-artifacts-test-{}", short_rand()));
        env.insert(ENV_DIR.to_string(), tmp.to_string_lossy().into_owned());

        let _a = Artifacts::init(&mut env);
        assert!(tmp.exists(), "artifacts dir should be created");
        assert_eq!(env.get(ENV_DIR).unwrap(), &tmp.to_string_lossy().to_string());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn sync_debounces_on_mtime() {
        let mut env = HashMap::new();
        let tmp = std::env::temp_dir().join(format!("wb-artifacts-debounce-{}", short_rand()));
        env.insert(ENV_DIR.to_string(), tmp.to_string_lossy().into_owned());

        let mut a = Artifacts::init(&mut env);
        let p = tmp.join("foo.json");
        fs::write(&p, "{}").unwrap();
        a.sync();
        // Second call should be a no-op (same mtime); we can't observe
        // uploads without a server, but we can check that `seen` tracked it.
        assert!(a.seen.contains_key(&p));
        let first_mtime = a.seen[&p];
        a.sync();
        assert_eq!(a.seen[&p], first_mtime, "mtime should not change without rewrite");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn content_type_by_extension() {
        assert_eq!(guess_content_type("orders.json"), "application/json");
        assert_eq!(guess_content_type("notes.txt"), "text/plain");
        assert_eq!(guess_content_type("x.bin"), "application/octet-stream");
    }

    #[test]
    fn url_encode_preserves_safe_chars() {
        assert_eq!(url_encode("cell-3-a1b2.json"), "cell-3-a1b2.json");
        assert_eq!(url_encode("orders.json"), "orders.json");
        assert_eq!(url_encode("foo bar"), "foo%20bar");
    }
}
