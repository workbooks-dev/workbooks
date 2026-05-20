// Step IR — stable identifiers for executable workbook steps.
//
// The legacy execution model keys per-block configuration (timeouts, retries,
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
//        "{include_chain}\0{position_le_bytes}\0{language}\0{first_64_bytes(body)}"
//      truncated to 12 hex chars.
//
// ## Compatibility shim
//
//   - `timeouts: {1: 30s}` still applies to "the first executable runtime block"
//     (excluding `{no-run}`). When `{#first} timeout=30s` is on that same block,
//     fence-attr wins; emit `wb-step-002` warning at validate time.
//   - `wb run workbook.md` produces identical behavior with or without the
//     step-IR layer present, as long as the workbook doesn't use any new fence
//     attrs.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

/// Stable identifier for an executable step. Survives edits, includes, and
/// reorderings as long as the user-supplied `{#id}` is preserved.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepId(pub String);

impl StepId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for StepId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Universal identifier vocabulary for fence attributes. Populated by
/// `parse_info_string` from a Pandoc-style attribute cluster
/// `{#id .class key=value}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FenceAttrs {
    /// Explicit id from `{#id}`. None means the id was hash-derived.
    pub explicit_id: Option<String>,
    /// `.tag` classes (Pandoc-style).
    pub classes: Vec<String>,
    /// Key/value attrs (`timeout=30s`, `retries=2`, `continue_on_error`).
    /// Bare `continue_on_error` is normalized to `continue_on_error=true`.
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

impl Span {
    pub fn point(line: u32) -> Self {
        Self {
            line,
            col: 1,
            len: 0,
            byte_offset: 0,
        }
    }
}

/// Where in the include tree this step came from. Tracked per-step so the run
/// loop can ask "what chain produced this step?" without re-walking the section
/// list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncludeFrame {
    /// Path string carried on `parser::Section::IncludeEnter` — relative to
    /// the CWD where `wb` was invoked when possible, falling back to absolute.
    pub id: String,
    pub title: Option<String>,
    /// Step position within the parent scope at which this include opened.
    /// Used as input to the position-hash for stable IDs.
    pub call_site: u32,
}

/// Origin metadata used to build a content-addressed step id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub file: PathBuf,
    /// Step position within the *current scope* (root or innermost include),
    /// 0-based. Hashed into auto-ids so two identical fenced blocks at
    /// different positions get different ids.
    pub position: u32,
}

/// One executable step in the resolved workbook. Replaces the (block_idx,
/// block_number) tuple as the canonical handle. A `Vec<Step>` is the new
/// surface for selective execution, callbacks, and validate's duplicate-id
/// detection. The legacy run loop still iterates `Section::Code | Browser`;
/// step IDs are looked up via `Workbook::build_steps()`.
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
        include_chain: &[IncludeFrame],
        position: u32,
        language: &str,
        body: &str,
        explicit_id: Option<&str>,
    ) -> StepId {
        if let Some(id) = explicit_id {
            return StepId(id.to_string());
        }
        let mut hasher = Sha256::new();
        for frame in include_chain {
            hasher.update(frame.id.as_bytes());
            hasher.update([0u8]);
        }
        hasher.update([0u8]);
        hasher.update(position.to_le_bytes());
        hasher.update([0u8]);
        hasher.update(language.as_bytes());
        hasher.update([0u8]);
        let body_prefix: Vec<u8> = body.bytes().take(64).collect();
        hasher.update(&body_prefix);
        let digest = hasher.finalize();
        // 12 hex chars = 48 bits. Documented in module header; revisit when
        // the cache (#18) cross-references step ids across workbooks.
        let hex: String = digest
            .iter()
            .take(6)
            .fold(String::with_capacity(12), |mut s, b| {
                use std::fmt::Write;
                let _ = write!(s, "{:02x}", b);
                s
            });
        StepId(format!("auto-{}", hex))
    }
}

/// Per-step execution policy, translated from the legacy block-number maps
/// and any `timeout=`/`retries=`/`continue_on_error` fence attrs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StepPolicy {
    /// Timeout for a single execution of this step. `None` = use the
    /// run-wide default.
    pub timeout_secs: Option<u64>,
    /// Retries *after* the first failure.
    pub retries: u32,
    /// If true, a failure of this step does NOT trigger `--bail`.
    pub continue_on_error: bool,
}

/// Result of resolving a single step's policy. Carries the fence-attr-vs-
/// legacy-map shadowing record so validate can emit `wb-step-002` warnings
/// without recomputing the comparison.
#[derive(Debug, Clone, Default)]
pub struct ResolvedStepPolicy {
    pub policy: StepPolicy,
    /// `(field_name, legacy_value)` for any field where a fence attr
    /// shadowed a legacy block-number map entry. `field_name` is one of
    /// `"timeout"`, `"retries"`, `"continue_on_error"`.
    pub shadowed_legacy: Vec<(&'static str, String)>,
}

/// Translate the legacy block-number-keyed maps into per-step config, with
/// fence attrs (`timeout=`, `retries=`, `continue_on_error`) winning over
/// the legacy map. The runtime block number is the position of the step
/// among executable steps (1-based), counting all steps regardless of
/// include scope — matches the existing `block_policy(u32)` contract.
pub fn resolve_step_policies(
    steps: &[Step],
    fm: &crate::parser::Frontmatter,
) -> Vec<ResolvedStepPolicy> {
    let mut out = Vec::with_capacity(steps.len());
    for (idx, step) in steps.iter().enumerate() {
        let block_number = (idx + 1) as u32;
        let legacy_timeout = fm
            .timeouts
            .as_ref()
            .and_then(|m| m.blocks.get(&block_number))
            .cloned();
        let legacy_retries = fm
            .retries
            .as_ref()
            .and_then(|m| m.get(&block_number))
            .copied();
        let legacy_continue = fm
            .continue_on_error
            .as_ref()
            .map(|v| v.contains(&block_number))
            .unwrap_or(false);

        let attr_timeout = step.attrs.kv.get("timeout").cloned();
        let attr_retries = step.attrs.kv.get("retries").cloned();
        let attr_continue = step
            .attrs
            .kv
            .get("continue_on_error")
            .map(|v| matches!(v.as_str(), "true" | "1" | "yes"))
            .unwrap_or(false);

        let mut shadowed = Vec::new();
        let timeout_secs = match attr_timeout.as_deref() {
            Some(s) => match crate::parser::parse_duration_secs(s) {
                Ok(n) => {
                    if let Some(legacy) = &legacy_timeout {
                        shadowed.push(("timeout", legacy.clone()));
                    }
                    Some(n)
                }
                Err(_) => legacy_timeout
                    .as_deref()
                    .and_then(|s| crate::parser::parse_duration_secs(s).ok()),
            },
            None => legacy_timeout
                .as_deref()
                .and_then(|s| crate::parser::parse_duration_secs(s).ok()),
        };

        let retries = match attr_retries.as_deref() {
            Some(s) => match s.parse::<u32>() {
                Ok(n) => {
                    if let Some(legacy) = legacy_retries {
                        shadowed.push(("retries", legacy.to_string()));
                    }
                    n
                }
                Err(_) => legacy_retries.unwrap_or(0),
            },
            None => legacy_retries.unwrap_or(0),
        };

        let continue_on_error = if step.attrs.kv.contains_key("continue_on_error") {
            if legacy_continue {
                shadowed.push(("continue_on_error", "true".to_string()));
            }
            attr_continue
        } else {
            legacy_continue
        };

        out.push(ResolvedStepPolicy {
            policy: StepPolicy {
                timeout_secs,
                retries,
                continue_on_error,
            },
            shadowed_legacy: shadowed,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(id: &str) -> IncludeFrame {
        IncludeFrame {
            id: id.to_string(),
            title: None,
            call_site: 0,
        }
    }

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

    #[test]
    fn explicit_id_takes_precedence() {
        let id = Step::compute_id(&[], 0, "bash", "echo hi", Some("login"));
        assert_eq!(id.0, "login");
    }

    #[test]
    fn auto_id_has_auto_prefix_and_12_hex_chars() {
        let id = Step::compute_id(&[], 0, "bash", "echo hi", None);
        assert!(id.0.starts_with("auto-"), "got: {}", id.0);
        let hex = &id.0["auto-".len()..];
        assert_eq!(hex.len(), 12);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn auto_id_is_deterministic() {
        let a = Step::compute_id(&[], 0, "bash", "echo hi", None);
        let b = Step::compute_id(&[], 0, "bash", "echo hi", None);
        assert_eq!(a, b);
    }

    #[test]
    fn auto_id_changes_with_position() {
        let a = Step::compute_id(&[], 0, "bash", "echo hi", None);
        let b = Step::compute_id(&[], 1, "bash", "echo hi", None);
        assert_ne!(a, b);
    }

    #[test]
    fn auto_id_changes_with_language() {
        let a = Step::compute_id(&[], 0, "bash", "echo hi", None);
        let b = Step::compute_id(&[], 0, "python", "echo hi", None);
        assert_ne!(a, b);
    }

    #[test]
    fn auto_id_changes_with_body() {
        let a = Step::compute_id(&[], 0, "bash", "echo hi", None);
        let b = Step::compute_id(&[], 0, "bash", "echo bye", None);
        assert_ne!(a, b);
    }

    #[test]
    fn auto_id_changes_with_include_chain() {
        let a = Step::compute_id(&[], 0, "bash", "echo hi", None);
        let b = Step::compute_id(&[frame("login.md")], 0, "bash", "echo hi", None);
        assert_ne!(a, b);
    }

    #[test]
    fn auto_id_handles_unicode_body() {
        // 64-byte truncation must not panic on a multi-byte char boundary.
        let body = "echo '日本語日本語日本語日本語日本語日本語日本語日本語日本語日本語'";
        let _id = Step::compute_id(&[], 0, "bash", body, None);
    }
}
