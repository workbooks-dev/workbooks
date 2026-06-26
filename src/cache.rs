//! Source-hash execution cache (#18).
//!
//! Opt-in via `--cache <id>`. When enabled, a block is skipped on re-run if its
//! source + parameter identity is byte-identical to a previously **successful**
//! run under the same cache id. This is the "skip unchanged blocks" memoization
//! that makes iterative agent re-runs fast: edit one block, re-run, and the
//! unchanged upstream blocks are skipped.
//!
//! Safety: caching is **opt-in at the run level** (`--cache`), can be disabled
//! per-run (`--no-cache`), and a side-effecting block can opt out with the
//! `{no-cache}` fence flag. A cached block is *skipped*, not replayed — its
//! stdout/outputs are not reproduced, so the cache is for idempotent pipelines
//! where skipping an unchanged step is safe.
//!
//! Cache key = sha256(language + body + param_hash + env_hash), where
//! `env_hash` is the env/secret identity (the wb-managed env minus `WB_*`
//! internals). Included files are already reflected in the block body (includes
//! expand into the step list). Runtime versions and artifact-input graphs are
//! the remaining follow-up (#46); change the cache id or pass `--no-cache` when
//! a runtime version changes.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// One cached block outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Whether the cached run succeeded. Only successes are consulted for skips.
    pub success: bool,
    pub exit_code: i32,
    pub cached_at: String,
}

/// A persisted per-id cache store at `~/.wb/cache/<id>.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStore {
    /// cache-key → entry.
    pub entries: HashMap<String, CacheEntry>,
}

/// Path to the cache file for an id.
pub fn cache_path(id: &str) -> PathBuf {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(".wb")
        .join("cache");
    base.join(format!("{}.json", sanitize(id)))
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

impl CacheStore {
    /// Load the store for `id`, or an empty store if absent/unreadable.
    pub fn load(id: &str) -> Self {
        match std::fs::read(cache_path(id)) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => CacheStore::default(),
        }
    }

    /// Persist the store. Best-effort; a write failure is returned for logging.
    pub fn save(&self, id: &str) -> std::io::Result<()> {
        let path = cache_path(id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    /// Is there a successful cached entry for this key?
    pub fn is_cached_success(&self, key: &str) -> bool {
        self.entries.get(key).is_some_and(|e| e.success)
    }

    /// Record a block's outcome under `key`, stamped `now` (rfc3339).
    pub fn record(&mut self, key: String, success: bool, exit_code: i32, now: &str) {
        self.entries.insert(
            key,
            CacheEntry {
                success,
                exit_code,
                cached_at: now.to_string(),
            },
        );
    }
}

/// Compute the cache key for a block:
/// sha256(language\0body\0param_hash\0env_hash), 16 hex. `env_hash` folds in the
/// env/secret identity (#18) so a block re-runs when the resolved env changes,
/// not just its source/params.
pub fn cache_key(language: &str, body: &str, param_hash: Option<&str>, env_hash: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(language.as_bytes());
    hasher.update([0u8]);
    hasher.update(body.as_bytes());
    hasher.update([0u8]);
    hasher.update(param_hash.unwrap_or("").as_bytes());
    hasher.update([0u8]);
    hasher.update(env_hash.as_bytes());
    let digest = hasher.finalize();
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// A stable digest of the env/secret identity that a cached block depends on:
/// the wb-managed env minus `WB_*` internals (which are run-specific, e.g.
/// `WB_ARTIFACTS_DIR`/`WB_OUT_*`, and would otherwise bust the cache every run).
/// Included files are already reflected in the block body (includes expand into
/// the step list), so they don't need separate keying; runtime versions and
/// artifact inputs remain a follow-up (#46).
pub fn env_identity(env: &HashMap<String, String>) -> String {
    let mut items: Vec<(&String, &String)> =
        env.iter().filter(|(k, _)| !k.starts_with("WB_")).collect();
    items.sort();
    let mut hasher = Sha256::new();
    for (k, v) in items {
        hasher.update(k.as_bytes());
        hasher.update(b"=");
        hasher.update(v.as_bytes());
        hasher.update(b"\n");
    }
    let digest = hasher.finalize();
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_changes_with_body_params_and_env() {
        let a = cache_key("bash", "echo 1", None, "e0");
        let b = cache_key("bash", "echo 2", None, "e0");
        let c = cache_key("bash", "echo 1", Some("ff00"), "e0");
        let d = cache_key("bash", "echo 1", None, "e1"); // env changed
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
        assert_eq!(a, cache_key("bash", "echo 1", None, "e0"));
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn env_identity_ignores_wb_internals() {
        let mut e1 = HashMap::new();
        e1.insert("REGION".to_string(), "us".to_string());
        e1.insert("WB_ARTIFACTS_DIR".to_string(), "/tmp/a".to_string());
        let mut e2 = e1.clone();
        e2.insert("WB_ARTIFACTS_DIR".to_string(), "/tmp/b".to_string()); // WB_* differs
        assert_eq!(env_identity(&e1), env_identity(&e2));
        e2.insert("REGION".to_string(), "eu".to_string()); // user env differs
        assert_ne!(env_identity(&e1), env_identity(&e2));
    }

    #[test]
    fn store_records_and_reads_success() {
        let mut s = CacheStore::default();
        let k = cache_key("bash", "echo hi", None, "e0");
        assert!(!s.is_cached_success(&k));
        s.record(k.clone(), true, 0, "2026-06-26T00:00:00Z");
        assert!(s.is_cached_success(&k));
        // A failed entry is not a cache hit.
        let k2 = cache_key("bash", "false", None, "e0");
        s.record(k2.clone(), false, 1, "2026-06-26T00:00:00Z");
        assert!(!s.is_cached_success(&k2));
    }

    #[test]
    fn roundtrip_serialization() {
        let mut s = CacheStore::default();
        s.record(cache_key("bash", "x", None, "e0"), true, 0, "t");
        let json = serde_json::to_string(&s).unwrap();
        let back: CacheStore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entries.len(), 1);
    }
}
