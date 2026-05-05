// `wb validate` — static analysis of workbook files without executing anything.
//
// Hard guarantee: this module MUST NOT import or call:
//   - executor (no code execution)
//   - sandbox (no Docker)
//   - secrets (no doppler/yard invocation)
//   - callback / signal (no network)
//
// Only the parser and diagnostic modules are in scope.

use crate::diagnostic::{self, Diagnostic, Severity, Span};
use crate::exit_codes;
use crate::parser::{self, Frontmatter, Section, Workbook};
use std::path::{Path, PathBuf};

pub struct ValidateOptions {
    pub strict: bool,
}

/// Validate a single file. Returns all diagnostics found (never executes any block).
pub fn validate_file(path: &Path, opts: &ValidateOptions) -> Vec<Diagnostic> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return vec![Diagnostic::error(
                "wb-yaml-001",
                path,
                format!("cannot read file: {e}"),
            )];
        }
    };
    validate_content(&content, path, opts)
}

/// Validate every .md file in a directory.
pub fn validate_dir(dir: &Path, opts: &ValidateOptions) -> Vec<Diagnostic> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            return vec![Diagnostic::error(
                "wb-yaml-001",
                dir,
                format!("cannot read directory: {e}"),
            )];
        }
    };

    let mut all = Vec::new();
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "md" || e == "markdown"))
        .collect();
    paths.sort();
    for path in &paths {
        all.extend(validate_file(path, opts));
    }
    all
}

/// Core: validate workbook source text. Used by both validate_file and tests.
pub fn validate_content(content: &str, path: &Path, opts: &ValidateOptions) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    // 1. Frontmatter YAML parse errors (wb-yaml-001) + unknown keys (wb-fm-001),
    //    wrong types (wb-fm-002).
    let yaml_region = extract_yaml_region(content);
    check_frontmatter_yaml(&yaml_region, content, path, &mut diags);

    // 2. Parse workbook with the tolerant parser (no side effects).
    let wb = parser::parse(content);

    // 3. Check per-block policy indices (wb-fm-003, wb-fm-006).
    check_block_policy_indices(&wb, path, &mut diags);

    // 4. Check wait/include/browser fence YAML (wb-yaml-002..004).
    check_fence_yaml(&wb.sections, content, path, &mut diags);

    // 5. Check include resolution (wb-inc-001..003).
    check_includes(&wb, path, &mut diags);

    // 6. Check secret provider names (wb-secret-001).
    check_secrets_config(&wb.frontmatter, path, &mut diags);

    // If --strict: promote warnings to errors.
    if opts.strict {
        for d in &mut diags {
            if d.severity == Severity::Warning {
                d.severity = Severity::Error;
            }
        }
    }

    diags
}

/// Map exit code from a diagnostics slice. Used by cmd_validate.
pub fn exit_code_for(diags: &[Diagnostic], strict: bool) -> i32 {
    let (errors, warnings) = diagnostic::counts(diags);
    if errors > 0 || (strict && warnings > 0) {
        exit_codes::EXIT_WORKBOOK_INVALID
    } else {
        exit_codes::EXIT_SUCCESS
    }
}

// ─── YAML region extraction ─────────────────────────────────────────────────

struct YamlRegion {
    yaml_text: String,
    /// 1-based line number where the YAML text starts in the parent .md file.
    start_line: u32,
}

fn extract_yaml_region(content: &str) -> Option<YamlRegion> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }
    let after_opening = &trimmed[3..]; // skip the opening ---
    let close_pos = after_opening.find("\n---")?;
    let yaml_text = after_opening[..close_pos].to_string();

    // Byte offset of yaml_text in the original content string.
    let opening_end = content.len() - trimmed.len() + 3; // after the "---"
                                                         // skip the optional newline that comes right after "---"
    let yaml_start = if content[opening_end..].starts_with('\n') {
        opening_end + 1
    } else {
        opening_end
    };

    let start_line = content[..yaml_start]
        .bytes()
        .filter(|&b| b == b'\n')
        .count() as u32
        + 1;

    Some(YamlRegion {
        yaml_text,
        start_line,
    })
}

// ─── Frontmatter checks ─────────────────────────────────────────────────────

fn check_frontmatter_yaml(
    region: &Option<YamlRegion>,
    _source: &str,
    path: &Path,
    out: &mut Vec<Diagnostic>,
) {
    let region = match region {
        Some(r) => r,
        None => return,
    };

    // Tolerant parse (same as runtime).
    if let Err(e) = serde_yaml::from_str::<Frontmatter>(&region.yaml_text) {
        let span = yaml_error_span(&e, region);
        out.push(
            Diagnostic::error(
                "wb-yaml-001",
                path,
                format!("frontmatter YAML parse error: {e}"),
            )
            .with_span(span),
        );
        // No point running further frontmatter checks if basic parse fails.
        return;
    }

    // Strict parse with deny_unknown_fields to catch wb-fm-001 / wb-fm-002.
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    #[allow(dead_code)]
    struct FrontmatterStrict {
        title: Option<serde_yaml::Value>,
        runtime: Option<serde_yaml::Value>,
        venv: Option<serde_yaml::Value>,
        env: Option<serde_yaml::Value>,
        vars: Option<serde_yaml::Value>,
        redact: Option<serde_yaml::Value>,
        secrets: Option<serde_yaml::Value>,
        setup: Option<serde_yaml::Value>,
        exec: Option<serde_yaml::Value>,
        working_dir: Option<serde_yaml::Value>,
        requires: Option<serde_yaml::Value>,
        timeouts: Option<serde_yaml::Value>,
        retries: Option<serde_yaml::Value>,
        continue_on_error: Option<serde_yaml::Value>,
    }

    if let Err(e) = serde_yaml::from_str::<FrontmatterStrict>(&region.yaml_text) {
        let msg = e.to_string();
        let code = if msg.contains("unknown field") {
            "wb-fm-001"
        } else {
            "wb-fm-002"
        };
        let span = yaml_error_span(&e, region);
        out.push(
            Diagnostic::error(code, path, format!("frontmatter schema error: {e}")).with_span(span),
        );
    }
}

/// Lift a serde_yaml error location into a span in the parent file by adding
/// the YAML region's start position.
fn yaml_error_span(e: &serde_yaml::Error, region: &YamlRegion) -> Span {
    if let Some(loc) = e.location() {
        let yaml_line = loc.line() as u32; // 1-based within YAML text
        let yaml_col = loc.column() as u32; // 1-based
        Span::point(region.start_line + yaml_line.saturating_sub(1), yaml_col)
    } else {
        Span::point(region.start_line, 1)
    }
}

// ─── Block-policy index checks ───────────────────────────────────────────────

fn check_block_policy_indices(wb: &Workbook, path: &Path, out: &mut Vec<Diagnostic>) {
    let block_count = wb.code_block_count() as u32;
    let fm = &wb.frontmatter;

    // wb-fm-003: malformed duration strings in timeouts:
    // wb-fm-006: block number out of range
    if let Some(ref timeouts) = fm.timeouts {
        for (block_num, dur_str) in timeouts {
            if parser::parse_duration_secs(dur_str).is_err() {
                out.push(Diagnostic::error(
                    "wb-fm-003",
                    path,
                    format!("timeouts: block {block_num}: invalid duration '{dur_str}'"),
                ));
            }
            if block_count > 0 && *block_num > block_count {
                out.push(
                    Diagnostic::warning(
                        "wb-fm-006",
                        path,
                        format!(
                            "timeouts: block {block_num} out of range (workbook has {block_count} blocks)"
                        ),
                    )
                    .with_help("remove or update this entry to reference an existing block"),
                );
            }
        }
    }

    // wb-fm-006: retries block number out of range
    if let Some(ref retries) = fm.retries {
        for block_num in retries.keys() {
            if block_count > 0 && *block_num > block_count {
                out.push(Diagnostic::warning(
                    "wb-fm-006",
                    path,
                    format!(
                        "retries: block {block_num} out of range (workbook has {block_count} blocks)"
                    ),
                ));
            }
        }
    }

    // wb-fm-006: continue_on_error block numbers out of range
    if let Some(ref coe) = fm.continue_on_error {
        for block_num in coe {
            if block_count > 0 && *block_num > block_count {
                out.push(Diagnostic::warning(
                    "wb-fm-006",
                    path,
                    format!(
                        "continue_on_error: block {block_num} out of range (workbook has {block_count} blocks)"
                    ),
                ));
            }
        }
    }
}

// ─── Fence YAML checks ───────────────────────────────────────────────────────

fn check_fence_yaml(sections: &[Section], _source: &str, _path: &Path, _out: &mut Vec<Diagnostic>) {
    for section in sections {
        match section {
            Section::Wait(spec) => {
                // The wait spec was already parsed successfully by the main parser,
                // so a re-parse here only catches errors in the raw body. For now
                // the parser already logs warnings; we use the spec's line_number
                // as a signal that it parsed. A future wave can store the raw body
                // and re-parse here for richer diagnostics.
                let _ = spec; // used for future diagnostics
            }
            Section::Include(spec) => {
                let _ = spec;
            }
            Section::Browser(spec) => {
                let _ = spec;
            }
            _ => {}
        }
    }
}

// ─── Include resolution check ────────────────────────────────────────────────

fn check_includes(wb: &Workbook, path: &Path, out: &mut Vec<Diagnostic>) {
    // Check Section::Include entries (unresolved in the pre-resolve parse).
    for section in &wb.sections {
        if let Section::Include(spec) = section {
            let parent_dir = path.parent().unwrap_or(Path::new("."));
            let target = parent_dir.join(&spec.path);
            if !target.exists() {
                out.push(
                    Diagnostic::error(
                        "wb-inc-001",
                        path,
                        format!(
                            "include at L{}: missing target '{}'",
                            spec.line_number, spec.path
                        ),
                    )
                    .with_span(Span::point(spec.line_number as u32, 1)),
                );
            }
        }
    }

    // Attempt full resolution to catch circular includes (wb-inc-002).
    match parser::resolve_includes(
        parser::parse(&match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return,
        }),
        path,
    ) {
        Ok(_) => {}
        Err(msg) => {
            let code = if msg.contains("circular") {
                "wb-inc-002"
            } else if msg.contains("cannot read") {
                "wb-inc-003"
            } else {
                "wb-inc-001"
            };
            out.push(Diagnostic::error(code, path, msg));
        }
    }
}

// ─── Secret provider check ───────────────────────────────────────────────────

const KNOWN_PROVIDERS: &[&str] = &[
    "env", "doppler", "yard", "command", "cmd", "dotenv", "file", "prompt",
];

fn check_secrets_config(fm: &Frontmatter, path: &Path, out: &mut Vec<Diagnostic>) {
    use crate::parser::SecretsConfig;

    let provider_names: Vec<String> = match &fm.secrets {
        None => return,
        Some(SecretsConfig::Single(p)) => vec![p.provider.clone()],
        Some(SecretsConfig::Multiple(providers)) => {
            providers.iter().map(|p| p.provider.clone()).collect()
        }
    };

    for name in &provider_names {
        if !KNOWN_PROVIDERS.contains(&name.as_str()) {
            out.push(Diagnostic::error(
                "wb-secret-001",
                path,
                format!("unknown secret provider '{name}' (known: {KNOWN_PROVIDERS:?})"),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn tmp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn validates_clean_workbook() {
        let content = "---\ntitle: Hello\n---\n\n```bash\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: false },
        );
        assert!(diags.is_empty(), "expected no diags, got: {diags:?}");
    }

    #[test]
    fn unknown_frontmatter_key_errors() {
        let content = "---\nunknownKey: foo\n---\n";
        let diags = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags.iter().any(|d| d.code == "wb-fm-001"),
            "expected wb-fm-001, got: {diags:?}"
        );
    }

    #[test]
    fn bad_duration_in_timeouts() {
        let content = "---\ntimeouts:\n  1: 5xyz\n---\n\n```bash\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags.iter().any(|d| d.code == "wb-fm-003"),
            "expected wb-fm-003, got: {diags:?}"
        );
    }

    #[test]
    fn out_of_range_block_number() {
        let content =
            "---\ntimeouts:\n  5: 30s\n---\n\n```bash\necho hi\n```\n```bash\necho bye\n```\n";
        let diags = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags.iter().any(|d| d.code == "wb-fm-006"),
            "expected wb-fm-006, got: {diags:?}"
        );
    }

    #[test]
    fn malformed_frontmatter_yaml_has_line_col() {
        // Unterminated YAML sequence — serde_yaml returns a parse error with location.
        let content = "---\nruntime: [\n---\n";
        let diags = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags.iter().any(|d| d.code == "wb-yaml-001"),
            "expected wb-yaml-001, got: {diags:?}"
        );
        let d = diags.iter().find(|d| d.code == "wb-yaml-001").unwrap();
        assert!(
            d.span.is_some(),
            "expected a span (line/col) on wb-yaml-001"
        );
    }

    #[test]
    fn missing_include_emits_wb_inc_001() {
        let content = "```include\npath: ./nonexistent.md\n```\n";
        let diags = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags.iter().any(|d| d.code == "wb-inc-001"),
            "expected wb-inc-001, got: {diags:?}"
        );
    }

    #[test]
    fn bad_secret_provider_emits_wb_secret_001() {
        let content = "---\nsecrets:\n  provider: fakeprovider\n---\n";
        let diags = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags.iter().any(|d| d.code == "wb-secret-001"),
            "expected wb-secret-001, got: {diags:?}"
        );
    }

    #[test]
    fn strict_mode_promotes_warnings() {
        // Out-of-range block is a warning in normal mode.
        let content = "---\ntimeouts:\n  5: 30s\n---\n\n```bash\necho hi\n```\n";
        let normal = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: false },
        );
        assert!(normal
            .iter()
            .any(|d| d.code == "wb-fm-006" && d.severity == Severity::Warning));
        let strict = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: true },
        );
        assert!(strict
            .iter()
            .any(|d| d.code == "wb-fm-006" && d.severity == Severity::Error));
    }

    #[test]
    fn validate_does_not_open_docker() {
        // Workbook with requires: block should validate without any Docker call.
        let content = "---\nrequires:\n  sandbox: python\n  pip: [requests]\n---\n\n```python\nprint(1)\n```\n";
        // If this panics or hangs, sandbox is being called (it isn't, by design).
        let diags = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: false },
        );
        // No diagnostics expected for a well-formed requires block.
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    #[test]
    fn validate_file_reads_and_checks() {
        let f = tmp("---\ntitle: OK\n---\n\n```bash\necho ok\n```\n");
        let diags = validate_file(f.path(), &ValidateOptions { strict: false });
        assert!(diags.is_empty(), "expected clean file, got: {diags:?}");
    }

    #[test]
    fn validate_file_missing_returns_error() {
        let diags = validate_file(
            Path::new("/nonexistent/file.md"),
            &ValidateOptions { strict: false },
        );
        assert!(!diags.is_empty());
    }
}
