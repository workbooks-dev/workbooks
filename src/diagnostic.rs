// STABILITY: codes in this module are part of the public CLI contract.
// Renaming a code is a breaking change for agents and scripts that key on them.
//
// Code namespaces:
//   wb-yaml-001  frontmatter YAML parse error (with line/col)
//   wb-yaml-002  wait fence YAML parse error
//   wb-yaml-003  include fence YAML parse error
//   wb-yaml-004  browser fence YAML parse error
//   wb-fm-001    unknown frontmatter key
//   wb-fm-002    wrong type in frontmatter value
//   wb-fm-003    bad duration string in timeouts:
//   wb-fm-004    retries: value not a u32 (reserved; falls out of fm-002)
//   wb-fm-005    continue_on_error: not a list of u32 (reserved; falls out of fm-002)
//   wb-fm-006    block-number map references block N but workbook has only M blocks
//   wb-inc-001   missing include target
//   wb-inc-002   circular include
//   wb-inc-003   unreadable include target
//   wb-attr-001  unknown fence attribute (deferred until step IR lands)
//   wb-secret-001 bad secret provider name
//   wb-step-001  duplicate explicit step id (deferred until step IR lands)

use serde::Serialize;
use std::path::PathBuf;

pub type Code = &'static str;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    // Reserved for informational notes in future diagnostics.
    #[allow(dead_code)]
    Note,
}

/// A position within a source file. Line and col are 1-based.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub len: u32,
    pub byte_offset: u32,
}

impl Span {
    pub fn point(line: u32, col: u32) -> Self {
        Self {
            line,
            col,
            len: 0,
            byte_offset: 0,
        }
    }

    /// Map a byte offset within a source string to a (line, col) span.
    /// Used to lift `serde_yaml::Location` into the parent .md file.
    #[allow(dead_code)]
    pub fn from_byte_offset(source: &str, offset: usize) -> Self {
        let clamped = offset.min(source.len());
        let before = &source[..clamped];
        let line = before.bytes().filter(|&b| b == b'\n').count() as u32 + 1;
        let col = match before.rfind('\n') {
            Some(pos) => (clamped - pos) as u32,
            None => clamped as u32 + 1,
        };
        Self {
            line,
            col,
            len: 0,
            byte_offset: offset as u32,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Code,
    pub message: String,
    pub span: Option<Span>,
    pub file: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn error(code: Code, file: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message: message.into(),
            span: None,
            file: file.into(),
            help: None,
        }
    }

    pub fn warning(code: Code, file: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code,
            message: message.into(),
            span: None,
            file: file.into(),
            help: None,
        }
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

/// Render diagnostics as human-readable text (rustc-style).
pub fn render_text(diags: &[Diagnostic]) -> String {
    let mut out = String::new();
    for d in diags {
        let sev = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        };
        let loc = if let Some(span) = d.span {
            format!("{}:{}:{}", d.file.display(), span.line, span.col)
        } else {
            d.file.display().to_string()
        };
        out.push_str(&format!("{}: {}[{}]: {}\n", loc, sev, d.code, d.message));
        if let Some(ref help) = d.help {
            out.push_str(&format!("   = help: {}\n", help));
        }
    }
    out
}

/// Render diagnostics as JSON (locked shape for agent consumption).
pub fn render_json(diags: &[Diagnostic]) -> String {
    let (errors, warnings) = counts(diags);
    let obj = serde_json::json!({
        "diagnostics": diags,
        "summary": { "errors": errors, "warnings": warnings }
    });
    serde_json::to_string_pretty(&obj).unwrap_or_default()
}

/// Count (errors, warnings) in a diagnostic slice.
pub fn counts(diags: &[Diagnostic]) -> (usize, usize) {
    let errors = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warnings = diags
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();
    (errors, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_from_byte_offset_basic() {
        let src = "line1\nline2\nline3\n";
        let sp = Span::from_byte_offset(src, 0);
        assert_eq!(sp.line, 1);
        assert_eq!(sp.col, 1);

        let sp2 = Span::from_byte_offset(src, 6); // start of "line2"
        assert_eq!(sp2.line, 2);
        assert_eq!(sp2.col, 1);

        let sp3 = Span::from_byte_offset(src, 8); // "ne2"
        assert_eq!(sp3.line, 2);
        assert_eq!(sp3.col, 3);
    }

    #[test]
    fn render_text_includes_code() {
        let d = Diagnostic::error("wb-yaml-001", "/some/file.md", "bad yaml")
            .with_span(Span::point(3, 1));
        let text = render_text(&[d]);
        assert!(
            text.contains("error[wb-yaml-001]"),
            "missing code in: {text}"
        );
        assert!(text.contains("bad yaml"), "missing message in: {text}");
        assert!(text.contains("3:1"), "missing location in: {text}");
    }

    #[test]
    fn render_json_shape_locked() {
        let d = Diagnostic::warning("wb-fm-001", "/f.md", "unknown key");
        let json = render_json(&[d]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v["diagnostics"].is_array());
        assert!(v["summary"]["errors"].is_number());
        assert!(v["summary"]["warnings"].is_number());
    }

    #[test]
    fn counts_separates_errors_and_warnings() {
        let diags = vec![
            Diagnostic::error("wb-yaml-001", "/f.md", "e1"),
            Diagnostic::error("wb-yaml-001", "/f.md", "e2"),
            Diagnostic::warning("wb-fm-001", "/f.md", "w1"),
        ];
        let (errors, warnings) = counts(&diags);
        assert_eq!(errors, 2);
        assert_eq!(warnings, 1);
    }
}
