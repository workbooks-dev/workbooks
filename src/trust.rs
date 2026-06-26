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
}
