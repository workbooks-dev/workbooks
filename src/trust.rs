//! Trust-on-first-use (TOFU) integrity gate for workbooks (part of #37).
//!
//! This is the same primitive as VS Code's Workspace Trust or `direnv allow`: a
//! local store records the sha256 of workbooks you've reviewed, and
//! `wb run --require-trust` refuses to execute a workbook whose content isn't
//! recorded (new file) or has changed since you trusted it (tampered/edited).
//!
//! **Scope, stated honestly:** this is an *integrity* check, not a signature. It
//! detects unexpected changes to a known-good workbook and forces a deliberate
//! `wb trust` review of new ones. It does **not** verify authorship and is not a
//! substitute for the cryptographic signing / remote-execution trust still
//! tracked under #37/#40 — do not rely on it to safely run untrusted
//! third-party workbooks.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The on-disk trust store: canonical workbook path → trusted sha256.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrustStore {
    #[serde(default)]
    pub entries: BTreeMap<String, String>,
}

/// Path to the trust store (`~/.wb/trust.json`, override `$WB_TRUST_PATH`).
pub fn trust_path() -> PathBuf {
    if let Some(p) = std::env::var_os("WB_TRUST_PATH") {
        return PathBuf::from(p);
    }
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join(".wb").join("trust.json")
}

/// Canonicalize a path for use as a stable store key (falls back to the given
/// path when the file doesn't exist yet).
pub fn canonical_key(file: &str) -> String {
    Path::new(file)
        .canonicalize()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| file.to_string())
}

/// sha256 of a file's bytes (hex). `None` on read error.
pub fn hash_file(file: &str) -> Option<String> {
    let bytes = std::fs::read(file).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Some(format!("{:x}", hasher.finalize()))
}

impl TrustStore {
    pub fn load() -> Self {
        match std::fs::read(trust_path()) {
            Ok(b) => serde_json::from_slice(&b).unwrap_or_default(),
            Err(_) => TrustStore::default(),
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = trust_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    /// Record (or refresh) a file's hash as trusted. Returns the hash.
    pub fn trust(&mut self, file: &str) -> Result<String, String> {
        let hash = hash_file(file).ok_or_else(|| format!("cannot read {file}"))?;
        self.entries.insert(canonical_key(file), hash.clone());
        Ok(hash)
    }

    pub fn remove(&mut self, file: &str) -> bool {
        self.entries.remove(&canonical_key(file)).is_some()
    }

    /// Trust status of a file's *current* content.
    pub fn status(&self, file: &str) -> TrustStatus {
        let Some(current) = hash_file(file) else {
            return TrustStatus::Unreadable;
        };
        match self.entries.get(&canonical_key(file)) {
            None => TrustStatus::Untrusted,
            Some(saved) if *saved == current => TrustStatus::Trusted,
            Some(_) => TrustStatus::Changed,
        }
    }
}

/// Outcome of checking a workbook against the trust store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustStatus {
    Trusted,
    /// Not in the store — never reviewed.
    Untrusted,
    /// In the store but the content changed since it was trusted.
    Changed,
    Unreadable,
}

impl TrustStatus {
    pub fn label(self) -> &'static str {
        match self {
            TrustStatus::Trusted => "trusted",
            TrustStatus::Untrusted => "untrusted",
            TrustStatus::Changed => "changed",
            TrustStatus::Unreadable => "unreadable",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str, body: &str) -> String {
        let dir = std::env::temp_dir().join(format!("wb-trust-{}-{}", name, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("wb.md");
        std::fs::write(&p, body).unwrap();
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn trust_then_detects_change() {
        let f = tmp("change", "echo a");
        let mut store = TrustStore::default();
        assert_eq!(store.status(&f), TrustStatus::Untrusted);
        store.trust(&f).unwrap();
        assert_eq!(store.status(&f), TrustStatus::Trusted);
        std::fs::write(&f, "echo b").unwrap();
        assert_eq!(store.status(&f), TrustStatus::Changed);
        assert!(store.remove(&f));
        assert_eq!(store.status(&f), TrustStatus::Untrusted);
    }

    #[test]
    fn roundtrip_serialization() {
        let mut store = TrustStore::default();
        store.entries.insert("/a.md".into(), "deadbeef".into());
        let json = serde_json::to_string(&store).unwrap();
        let back: TrustStore = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.entries.get("/a.md").map(String::as_str),
            Some("deadbeef")
        );
    }

    // Serialize tests that mutate the shared WB_TRUST_PATH / HOME env vars.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn trust_path_override_and_home_fallback() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("WB_TRUST_PATH", "/custom/trust.json");
        assert_eq!(trust_path(), PathBuf::from("/custom/trust.json"));
        std::env::remove_var("WB_TRUST_PATH");

        let prev_home = std::env::var_os("HOME");
        std::env::set_var("HOME", "/home/tester");
        assert_eq!(
            trust_path(),
            PathBuf::from("/home/tester").join(".wb").join("trust.json")
        );
        // No HOME, no override → temp_dir base.
        std::env::remove_var("HOME");
        assert_eq!(
            trust_path(),
            std::env::temp_dir().join(".wb").join("trust.json")
        );
        if let Some(h) = prev_home {
            std::env::set_var("HOME", h)
        }
    }

    #[test]
    fn save_load_roundtrip_on_disk() {
        let _g = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        // Nested non-existent parent to exercise create_dir_all.
        let path = dir.path().join("nested").join("trust.json");
        let prev = std::env::var_os("WB_TRUST_PATH");
        std::env::set_var("WB_TRUST_PATH", &path);

        let f = tmp("ondisk", "echo hi");
        let mut store = TrustStore::default();
        store.trust(&f).unwrap();
        store.save().unwrap();
        assert!(path.exists());

        let loaded = TrustStore::load();
        assert_eq!(loaded.status(&f), TrustStatus::Trusted);

        match prev {
            Some(p) => std::env::set_var("WB_TRUST_PATH", p),
            None => std::env::remove_var("WB_TRUST_PATH"),
        }
    }

    #[test]
    fn save_create_dir_all_failure_is_error() {
        // WB_TRUST_PATH whose parent can't be created (a file blocks the dir)
        // makes save's create_dir_all fail and propagate (line 67-68 error arm).
        let _g = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, b"i am a file").unwrap();
        let path = blocker.join("nested").join("trust.json");
        let prev = std::env::var_os("WB_TRUST_PATH");
        std::env::set_var("WB_TRUST_PATH", &path);

        let err = TrustStore::default().save();
        assert!(err.is_err(), "save under a file path should error");

        match prev {
            Some(p) => std::env::set_var("WB_TRUST_PATH", p),
            None => std::env::remove_var("WB_TRUST_PATH"),
        }
    }

    #[test]
    fn save_with_parentless_path_skips_create_dir() {
        // A path with no parent ("/") takes the `if let Some(parent)` false
        // arm (line 68), skipping create_dir_all; the write to root then fails.
        let _g = ENV_LOCK.lock().unwrap();
        let prev = std::env::var_os("WB_TRUST_PATH");
        std::env::set_var("WB_TRUST_PATH", "/");
        let res = TrustStore::default().save();
        match prev {
            Some(p) => std::env::set_var("WB_TRUST_PATH", p),
            None => std::env::remove_var("WB_TRUST_PATH"),
        }
        assert!(res.is_err(), "writing trust store to / should fail");
    }

    #[test]
    fn load_missing_file_is_default() {
        let _g = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.json");
        let prev = std::env::var_os("WB_TRUST_PATH");
        std::env::set_var("WB_TRUST_PATH", &path);
        assert!(TrustStore::load().entries.is_empty());
        match prev {
            Some(p) => std::env::set_var("WB_TRUST_PATH", p),
            None => std::env::remove_var("WB_TRUST_PATH"),
        }
    }

    #[test]
    fn unreadable_file_status_and_trust_error() {
        let missing = "/no/such/wb-trust-file.md";
        let store = TrustStore::default();
        assert_eq!(store.status(missing), TrustStatus::Unreadable);
        assert!(hash_file(missing).is_none());
        let mut store = TrustStore::default();
        let err = store.trust(missing).unwrap_err();
        assert!(err.contains("cannot read"));
    }

    #[test]
    fn remove_absent_returns_false() {
        let mut store = TrustStore::default();
        assert!(!store.remove("/never/added.md"));
    }

    #[test]
    fn status_labels_cover_all_variants() {
        assert_eq!(TrustStatus::Trusted.label(), "trusted");
        assert_eq!(TrustStatus::Untrusted.label(), "untrusted");
        assert_eq!(TrustStatus::Changed.label(), "changed");
        assert_eq!(TrustStatus::Unreadable.label(), "unreadable");
    }

    #[test]
    fn canonical_key_falls_back_for_missing_path() {
        // A non-existent path cannot canonicalize; key is the original string.
        assert_eq!(canonical_key("/no/such/path.md"), "/no/such/path.md");
    }
}
