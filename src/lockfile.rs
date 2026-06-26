//! Reproducibility lockfile (#47, the unsigned half).
//!
//! `wb lock <file>` records the input identity of a workbook — a sha256 per
//! executable step (over language + body) — into `<file>.lock`. `wb run
//! --locked` recomputes and refuses to run if the inputs drifted (a step was
//! added, removed, or edited) since the lockfile was written. This is the
//! `Cargo.lock`/`package-lock.json` analogue for runbooks: commit the lock and
//! CI fails loudly when the runbook (or an included file, which expands into the
//! step list) changes unexpectedly.
//!
//! Cryptographically **signed** run attestations are a separate, still-open
//! piece of #47 (they need a signing key); this is integrity, not authorship.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::step_ir::Step;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockedStep {
    pub step_id: String,
    pub language: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    pub version: u32,
    pub workbook: String,
    pub steps: Vec<LockedStep>,
}

/// Default lockfile path for a workbook: `<file>.lock`.
pub fn lock_path(file: &str, explicit: Option<&str>) -> PathBuf {
    match explicit {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from(format!("{file}.lock")),
    }
}

fn step_hash(step: &Step) -> String {
    let mut hasher = Sha256::new();
    hasher.update(step.language.as_bytes());
    hasher.update([0u8]);
    hasher.update(step.body.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Build a lockfile snapshot from a workbook's resolved (post-include) steps.
pub fn build(file: &str, steps: &[Step]) -> Lockfile {
    Lockfile {
        version: 1,
        workbook: file.to_string(),
        steps: steps
            .iter()
            .map(|s| LockedStep {
                step_id: s.id.as_str().to_string(),
                language: s.language.clone(),
                sha256: step_hash(s),
            })
            .collect(),
    }
}

pub fn load(path: &Path) -> Result<Lockfile, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("read lockfile {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse lockfile {}: {e}", path.display()))
}

pub fn save(path: &Path, lock: &Lockfile) -> Result<(), String> {
    let json = serde_json::to_string_pretty(lock).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| format!("write lockfile {}: {e}", path.display()))
}

/// Verify the current steps against a lockfile. Returns a human-readable drift
/// description on mismatch, or `Ok(())` if the inputs are identical.
pub fn verify(locked: &Lockfile, current: &[Step]) -> Result<(), String> {
    let current = build(&locked.workbook, current);
    if current.steps.len() != locked.steps.len() {
        return Err(format!(
            "step count changed: locked {} vs current {}",
            locked.steps.len(),
            current.steps.len()
        ));
    }
    for (i, (l, c)) in locked.steps.iter().zip(current.steps.iter()).enumerate() {
        if l.sha256 != c.sha256 || l.language != c.language {
            return Err(format!(
                "step {} ('{}') changed since the lockfile was written",
                i + 1,
                l.step_id
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    fn steps(md: &str) -> Vec<Step> {
        parser::parse(md).build_steps()
    }

    #[test]
    fn lock_matches_unchanged_and_detects_edits() {
        let md = "---\nruntime: bash\n---\n```bash\necho a\n```\n```bash\necho b\n```\n";
        let lock = build("w.md", &steps(md));
        assert_eq!(lock.steps.len(), 2);
        // Unchanged → ok.
        assert!(verify(&lock, &steps(md)).is_ok());
        // Edit a block → drift.
        let edited = "---\nruntime: bash\n---\n```bash\necho a\n```\n```bash\necho CHANGED\n```\n";
        assert!(verify(&lock, &steps(edited)).is_err());
        // Remove a block → count drift.
        let fewer = "---\nruntime: bash\n---\n```bash\necho a\n```\n";
        assert!(verify(&lock, &steps(fewer)).is_err());
    }

    #[test]
    fn roundtrip() {
        let lock = Lockfile {
            version: 1,
            workbook: "w.md".into(),
            steps: vec![LockedStep {
                step_id: "x".into(),
                language: "bash".into(),
                sha256: "ff".into(),
            }],
        };
        let json = serde_json::to_string(&lock).unwrap();
        let back: Lockfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.steps, lock.steps);
    }
}
