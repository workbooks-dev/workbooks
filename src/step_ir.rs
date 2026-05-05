// DESIGN ONLY — types are reserved; implementations land in the next wave.
// See TODO.md #13 / #29.
// The `#[allow(dead_code)]` below is intentional — this module holds the
// design-reserved shape. Consumers are added in the implementation wave.
#![allow(dead_code)]
//
// LATER: see TODO.md #13/#29 — implementation pending.
//
// The current execution model keys per-block configuration (timeouts, retries,
// continue_on_error) on 1-based block number. That number is brittle: editing
// the workbook to insert a block silently shifts every downstream key. Stable
// step IDs replace block-number indexing as the canonical reference for
// per-block config, checkpoints, callbacks, cache entries, selective runs,
// and docs.
//
// ## ID rules
//
//   1. If `attrs.explicit_id` is Some, use `StepId(explicit_id.clone())`.
//      Duplicate ids are a validate error (`wb-step-001`).
//   2. Otherwise hash the include chain ids + position-within-parent +
//      language + body-prefix into a short hex string with prefix `auto-`.
//      Deterministic so the same workbook produces the same auto ids on
//      every parse.
//
//      Hash function: SHA-256 of
//        "{include_chain}\0{position}\0{language}\0{first_64_bytes(body)}",
//      truncated to 12 hex chars.
//
// ## Compatibility shim
//
//   - `timeouts: {1: 30s}` still applies to "the first executable runtime block"
//     (excluding `{no-run}`). When `{#first} timeout=30s` is on that same block,
//     fence-attr wins; emit `wb-step-002` warning.
//   - `wb run workbook.md` produces identical behavior with or without the
//     step-IR layer present, as long as the workbook doesn't use any new fence
//     attrs.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Stable identifier for an executable step. Survives edits, includes, and
/// reorderings as long as the user-supplied `{#id}` is preserved.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepId(pub String);

/// Universal identifier vocabulary for fence attributes. Currently only `id`
/// is designed; tags / classes / kv attrs land with #13.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FenceAttrs {
    /// Explicit id from `{#id}`. None means the id was hash-derived.
    pub explicit_id: Option<String>,
    /// `.tag` classes (Pandoc-style).
    pub classes: Vec<String>,
    /// Key/value attrs (`timeout=30s`, `retries=2`, `continue_on_error`).
    /// Values are stored as strings; the Frontmatter compatibility shim
    /// (see below) populates this from the existing maps.
    pub kv: std::collections::BTreeMap<String, String>,
}

/// Source span for editor integrations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub len: u32,
    pub byte_offset: u32,
}

/// Where in the include tree this step came from. Tracked per-step so the run
/// loop can ask "what chain produced this step?" without re-walking the section
/// list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncludeFrame {
    /// Path relative to id_root, mirrors parser::IncludeFrame.
    pub id: String,
    pub title: Option<String>,
    /// Position within the parent's section list at which this include opened.
    /// Used as input to the position-hash for stable IDs.
    pub call_site: u32,
}

/// Origin metadata used to build a content-addressed step id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub file: PathBuf,
    /// Position within the *current* file's section list (0-based).
    /// Hashed into the step id so two identical fenced blocks at different
    /// positions get different ids.
    pub position: u32,
}

/// One executable step in the resolved workbook. Replaces the (block_idx,
/// block_number) tuple as the canonical handle. A `Vec<Step>` replaces the
/// today's filtered iteration over `Section::Code | Section::Browser`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub id: StepId,
    pub attrs: FenceAttrs,
    pub span: Span,
    pub source: Source,
    /// "bash", "python", "browser", ...
    pub language: String,
    /// Code text or browser slice raw YAML.
    pub body: String,
    pub include_chain: Vec<IncludeFrame>,
}

impl Step {
    /// Compute a stable step id. See module-level doc for the two rules.
    pub fn compute_id(
        _include_chain: &[IncludeFrame],
        _position: u32,
        _language: &str,
        _body: &str,
        _explicit_id: Option<&str>,
    ) -> StepId {
        unimplemented!(
            "step id computation is deferred to the next wave; \
             see TODO.md #13/#29"
        )
    }
}

/// Per-step execution policy, translated from the legacy block-number maps.
pub struct StepPolicy {
    pub timeout_secs: Option<u64>,
    pub retries: u32,
    pub continue_on_error: bool,
}

/// Translate the existing block-number-keyed maps into per-step config.
/// Runs once at parse time; the run loop reads `step.policy()` instead of
/// calling `frontmatter.block_policy(block_number)`.
///
/// ## Compatibility shim
/// Legacy `timeouts: {1: 30s}` maps the *1-based runtime block number*
/// (which excludes {no-run} blocks but includes browser slices). For each
/// step in `steps`, compute its 1-based runtime block number and look it up
/// in the legacy map. Same for retries and continue_on_error.
/// Future fence-attr `timeout=30s` on the step itself wins over the legacy
/// map. Conflict resolution: warn via diagnostic, fence-attr wins.
pub fn resolve_step_policies(_steps: &[Step], _fm: &crate::parser::Frontmatter) -> Vec<StepPolicy> {
    unimplemented!(
        "step policy resolution is deferred to the next wave; \
         see TODO.md #13/#29"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_id_is_serializable() {
        let id = StepId("auto-abc123def456".to_string());
        let json = serde_json::to_string(&id).unwrap();
        let back: StepId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn fence_attrs_default_is_all_empty() {
        let attrs = FenceAttrs::default();
        assert!(attrs.explicit_id.is_none());
        assert!(attrs.classes.is_empty());
        assert!(attrs.kv.is_empty());
    }
}
