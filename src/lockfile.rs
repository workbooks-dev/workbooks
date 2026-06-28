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

/// Execution-relevant fence attributes that change *whether* or *when* a step
/// runs, independent of its language + body. Folded into the per-step hash so a
/// `{no-run}`→live edit (or a `{when=}`/`{skip_if=}` change) is detected as
/// drift by `--locked`. Without this, flipping an inert documentation block into
/// executed code (or re-gating a live block) slips past the lockfile because the
/// language + body bytes are unchanged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StepExecAttrs {
    /// `{no-run}`: block is parsed/rendered but never executed.
    pub skip_execution: bool,
    /// `{when=EXPR}`: runtime-conditional execution.
    pub when: Option<String>,
    /// `{skip_if=EXPR}`: inverse of `when`.
    pub skip_if: Option<String>,
}

/// Extract the execution-relevant attrs for each resolved step, in the same
/// order as `Workbook::build_steps()` (Code + Browser sections, includes
/// expanded). The returned vec is index-aligned with that step list, so it can
/// be passed alongside it to [`build`]/[`verify`].
pub fn exec_attrs(workbook: &crate::parser::Workbook) -> Vec<StepExecAttrs> {
    use crate::parser::Section;
    let mut out = Vec::new();
    for section in &workbook.sections {
        match section {
            Section::Code(b) => out.push(StepExecAttrs {
                skip_execution: b.skip_execution,
                when: b.when.clone(),
                skip_if: b.skip_if.clone(),
            }),
            Section::Browser(spec) => out.push(StepExecAttrs {
                skip_execution: spec.skip_execution,
                when: spec.when.clone(),
                skip_if: spec.skip_if.clone(),
            }),
            _ => {}
        }
    }
    out
}

/// Default lockfile path for a workbook: `<file>.lock`.
pub fn lock_path(file: &str, explicit: Option<&str>) -> PathBuf {
    match explicit {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from(format!("{file}.lock")),
    }
}

fn step_hash(step: &Step, exec: &StepExecAttrs) -> String {
    let mut hasher = Sha256::new();
    hasher.update(step.language.as_bytes());
    hasher.update([0u8]);
    hasher.update(step.body.as_bytes());
    hasher.update([0u8]);
    // Fold in the execution-relevant fence attrs so a `{no-run}`→live edit or a
    // `{when=}`/`{skip_if=}` change is caught even when language + body are
    // byte-identical (see `StepExecAttrs`).
    hasher.update([exec.skip_execution as u8]);
    hasher.update([0u8]);
    hasher.update(exec.when.as_deref().unwrap_or("").as_bytes());
    hasher.update([0u8]);
    hasher.update(exec.skip_if.as_deref().unwrap_or("").as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Build a lockfile snapshot from a workbook's resolved (post-include) steps.
/// `exec` is the index-aligned execution-attr list from [`exec_attrs`]; a short
/// or empty slice falls back to defaults for the missing tail.
pub fn build(file: &str, steps: &[Step], exec: &[StepExecAttrs]) -> Lockfile {
    let default = StepExecAttrs::default();
    Lockfile {
        version: 1,
        workbook: file.to_string(),
        steps: steps
            .iter()
            .enumerate()
            .map(|(i, s)| LockedStep {
                step_id: s.id.as_str().to_string(),
                language: s.language.clone(),
                sha256: step_hash(s, exec.get(i).unwrap_or(&default)),
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
pub fn verify(locked: &Lockfile, current: &[Step], exec: &[StepExecAttrs]) -> Result<(), String> {
    let current = build(&locked.workbook, current, exec);
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

    fn execs(md: &str) -> Vec<StepExecAttrs> {
        exec_attrs(&parser::parse(md))
    }

    fn build_md(file: &str, md: &str) -> Lockfile {
        build(file, &steps(md), &execs(md))
    }

    fn verify_md(lock: &Lockfile, md: &str) -> Result<(), String> {
        verify(lock, &steps(md), &execs(md))
    }

    #[test]
    fn lock_matches_unchanged_and_detects_edits() {
        let md = "---\nruntime: bash\n---\n```bash\necho a\n```\n```bash\necho b\n```\n";
        let lock = build_md("w.md", md);
        assert_eq!(lock.steps.len(), 2);
        // Unchanged → ok.
        assert!(verify_md(&lock, md).is_ok());
        // Edit a block → drift.
        let edited = "---\nruntime: bash\n---\n```bash\necho a\n```\n```bash\necho CHANGED\n```\n";
        assert!(verify_md(&lock, edited).is_err());
        // Remove a block → count drift.
        let fewer = "---\nruntime: bash\n---\n```bash\necho a\n```\n";
        assert!(verify_md(&lock, fewer).is_err());
    }

    #[test]
    fn toggling_no_run_changes_hash_and_is_detected_as_drift() {
        // A `{no-run}` block is inert documentation; removing the flag turns it
        // into executed code. Language + body are byte-identical, so the only
        // signal is the folded-in exec attr.
        let inert = "---\nruntime: bash\n---\n```bash {no-run}\nrm -rf /\n```\n";
        let live = "---\nruntime: bash\n---\n```bash\nrm -rf /\n```\n";

        let inert_lock = build_md("w.md", inert);
        let live_lock = build_md("w.md", live);
        // The per-step hash must differ between inert and live.
        assert_ne!(inert_lock.steps[0].sha256, live_lock.steps[0].sha256);

        // A lockfile written against the inert block must reject the live one.
        assert!(verify_md(&inert_lock, inert).is_ok());
        assert!(
            verify_md(&inert_lock, live).is_err(),
            "{{no-run}}→live edit must be detected as drift"
        );
    }

    #[test]
    fn toggling_when_or_skip_if_is_detected_as_drift() {
        let base = "---\nruntime: bash\n---\n```bash\necho hi\n```\n";
        let gated = "---\nruntime: bash\n---\n```bash {when=$DEPLOY}\necho hi\n```\n";
        let skip = "---\nruntime: bash\n---\n```bash {skip_if=$DRY}\necho hi\n```\n";
        let lock = build_md("w.md", base);
        assert!(verify_md(&lock, base).is_ok());
        assert!(verify_md(&lock, gated).is_err(), "adding when= is drift");
        assert!(verify_md(&lock, skip).is_err(), "adding skip_if= is drift");
    }

    #[test]
    fn lock_path_default_and_explicit() {
        assert_eq!(
            lock_path("deploy.md", None),
            PathBuf::from("deploy.md.lock")
        );
        assert_eq!(
            lock_path("deploy.md", Some("/tmp/custom.lock")),
            PathBuf::from("/tmp/custom.lock")
        );
    }

    #[test]
    fn save_then_load_roundtrip_on_disk() {
        let md = "---\nruntime: bash\n---\n```bash\necho a\n```\n";
        let lock = build_md("w.md", md);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.md.lock");
        save(&path, &lock).unwrap();
        let loaded = load(&path).unwrap();
        assert!(verify_md(&loaded, md).is_ok());
    }

    #[test]
    fn load_missing_and_malformed_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("absent.lock");
        let err = load(&missing).unwrap_err();
        assert!(err.contains("read lockfile"));

        let bad = dir.path().join("bad.lock");
        std::fs::write(&bad, b"{not json").unwrap();
        let err = load(&bad).unwrap_err();
        assert!(err.contains("parse lockfile"));
    }

    #[test]
    fn verify_detects_language_change() {
        // Same body, different language: hits the `l.language != c.language`
        // (and sha) drift branch.
        let bash = "---\nruntime: bash\n---\n```bash\nprint(1)\n```\n";
        let py = "---\nruntime: bash\n---\n```python\nprint(1)\n```\n";
        let lock = build_md("w.md", bash);
        assert!(verify_md(&lock, py).is_err());
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
