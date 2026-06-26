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

    // 7. Check step ids: duplicate explicit ids (wb-step-001) and fence-attr/
    //    legacy-map shadowing (wb-step-002).
    check_step_ids(&wb, path, &mut diags);

    // 8. Workflow metadata is opaque to the runner, but declared nodes should
    //    line up with executable step ids so callbacks can be correlated.
    check_workflow_nodes(&wb, path, &mut diags);

    // 9. Unknown fence attributes (wb-attr-001). Now that the vocabulary is
    //    closed, flag typo'd flags / keys the runtime would silently ignore.
    check_fence_attrs(&wb, path, &mut diags);

    // 10. Typed parameter declarations (wb-param-001/002).
    check_params(&wb, path, &mut diags);

    // 11. Inline assertion fences (wb-expect-001).
    check_expects(&wb, path, &mut diags);

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
        required: Option<serde_yaml::Value>,
        workflow: Option<serde_yaml::Value>,
        timeouts: Option<serde_yaml::Value>,
        retries: Option<serde_yaml::Value>,
        continue_on_error: Option<serde_yaml::Value>,
        params: Option<serde_yaml::Value>,
        profiles: Option<serde_yaml::Value>,
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
        if let Some(dur_str) = timeouts.default.as_deref() {
            if parser::parse_duration_secs(dur_str).is_err() {
                out.push(Diagnostic::error(
                    "wb-fm-003",
                    path,
                    format!("timeouts: _default: invalid duration '{dur_str}'"),
                ));
            }
        }
        for (block_num, dur_str) in &timeouts.blocks {
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
    let parent_dir = path.parent().unwrap_or(Path::new("."));

    // Check Section::Include entries (unresolved in the pre-resolve parse).
    for section in &wb.sections {
        if let Section::Include(spec) = section {
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

    // Frontmatter `required:` entries (synthesized into Section::Include during
    // resolve_includes, so the loop above doesn't see them yet).
    if let Some(reqs) = &wb.frontmatter.required {
        for req in reqs {
            let target = parent_dir.join(req);
            if !target.exists() {
                out.push(Diagnostic::error(
                    "wb-inc-001",
                    path,
                    format!("required '{req}': missing target"),
                ));
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
        Err(err) => {
            let msg = err.message();
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

// ─── Step-id checks (wb-step-001, wb-step-002) ───────────────────────────────

fn check_step_ids(wb: &Workbook, path: &Path, out: &mut Vec<Diagnostic>) {
    use crate::step_ir;
    use std::collections::HashMap;

    let steps = wb.build_steps();

    // wb-step-001: duplicate explicit ids. Auto-derived ids are deterministic
    // but include position+body, so accidental collisions there are extremely
    // unlikely; if they did occur it would be a hash bug, not a user error.
    let mut seen: HashMap<String, Vec<u32>> = HashMap::new();
    for step in &steps {
        if let Some(explicit) = step.attrs.explicit_id.as_ref() {
            seen.entry(explicit.clone())
                .or_default()
                .push(step.span.line);
        }
    }
    for section in &wb.sections {
        if let crate::parser::Section::Wait(spec) = section {
            if let Some(explicit) = spec.attrs.explicit_id.as_ref() {
                seen.entry(explicit.clone())
                    .or_default()
                    .push(spec.line_number as u32);
            }
        }
    }
    for (id, lines) in seen {
        if lines.len() > 1 {
            let line_labels: Vec<String> = lines.iter().map(|line| format!("L{}", line)).collect();
            out.push(
                Diagnostic::error(
                    "wb-step-001",
                    path,
                    format!(
                        "duplicate step id '{id}' on {} workflow nodes ({})",
                        lines.len(),
                        line_labels.join(", ")
                    ),
                )
                .with_help("rename one of the colliding `{#id}` attrs"),
            );
        }
    }

    // wb-step-002: fence-attr policy shadows a legacy block-number entry. The
    // fence attr wins; emit a warning so users can decide which to keep.
    let resolved = step_ir::resolve_step_policies(&steps, &wb.frontmatter);
    for (idx, r) in resolved.iter().enumerate() {
        for (field, legacy_value) in &r.shadowed_legacy {
            let line = steps[idx].span.line;
            out.push(
                Diagnostic::warning(
                    "wb-step-002",
                    path,
                    format!(
                        "block at L{line}: fence attr `{field}=` shadows legacy `{}: {{{}: {}}}`",
                        field_to_legacy_key(field),
                        idx + 1,
                        legacy_value
                    ),
                )
                .with_help("remove the legacy entry; the fence attr is the source of truth"),
            );
        }
    }
}

fn field_to_legacy_key(field: &str) -> &str {
    match field {
        "timeout" => "timeouts",
        _ => field,
    }
}

// ─── Fence-attribute checks (wb-attr-001) ────────────────────────────────────

/// Key/value fence attrs the runtime acts on. Anything else in `attrs.kv` is a
/// typo or an attr from a newer wb than the one validating — either way the
/// runtime ignores it silently, which is exactly what `wb validate` exists to
/// surface. `when=` / `skip_if=` are pulled into dedicated fields by the parser
/// and never reach `kv`, so they don't need to be listed here.
const KNOWN_KV_ATTRS: &[&str] = &["timeout", "retries", "continue_on_error", "reads"];

fn check_fence_attrs(wb: &Workbook, path: &Path, out: &mut Vec<Diagnostic>) {
    for step in wb.build_steps() {
        let line = step.span.line;
        for key in step.attrs.kv.keys() {
            if !KNOWN_KV_ATTRS.contains(&key.as_str()) {
                out.push(
                    Diagnostic::warning(
                        "wb-attr-001",
                        path,
                        format!("block at L{line}: unknown fence attribute `{key}=`"),
                    )
                    .with_span(Span::point(line, 1))
                    .with_help(format!(
                        "known attrs: {}. the runtime ignores unknown attrs",
                        KNOWN_KV_ATTRS.join(", ")
                    )),
                );
            }
        }
        for flag in &step.attrs.unknown {
            out.push(
                Diagnostic::warning(
                    "wb-attr-001",
                    path,
                    format!("block at L{line}: unknown fence flag `{flag}`"),
                )
                .with_span(Span::point(line, 1))
                .with_help("known flags: no-run, silent, no-cache, continue_on_error"),
            );
        }
    }
}

/// Render a YAML scalar for messages / comparison.
fn scalar_str(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// wb-param-001: bad parameter declaration (unknown type, default/type
/// mismatch, default not in one_of). wb-param-002: a profile references an
/// undeclared param or a value that violates its param's type/choices.
fn check_params(wb: &Workbook, path: &Path, out: &mut Vec<Diagnostic>) {
    let Some(params) = wb.frontmatter.params.as_ref() else {
        // A profile with no params: block is dead config; flag each profile key.
        if let Some(profiles) = wb.frontmatter.profiles.as_ref() {
            for (pname, block) in profiles {
                for key in block.keys() {
                    out.push(Diagnostic::warning(
                        "wb-param-002",
                        path,
                        format!("profile '{pname}' sets '{key}' but no params are declared"),
                    ));
                }
            }
        }
        return;
    };

    // Validate each declared param's type + default.
    for (name, spec) in params {
        let def = spec.to_def();
        if let Some(t) = def.type_.as_deref() {
            if !crate::params::KNOWN_TYPES.contains(&t) {
                out.push(
                    Diagnostic::error(
                        "wb-param-001",
                        path,
                        format!("param '{name}': unknown type '{t}'"),
                    )
                    .with_help(format!(
                        "known types: {}",
                        crate::params::KNOWN_TYPES.join(", ")
                    )),
                );
            }
        }
        if let Some(default) = def.default.as_ref().and_then(scalar_str) {
            check_param_value(name, &def, &default, "default", "wb-param-001", path, out);
        }
        // enum/one_of sanity: an `enum` type with no choices can never match.
        if def.type_.as_deref() == Some("enum") && def.one_of.is_empty() {
            out.push(Diagnostic::warning(
                "wb-param-001",
                path,
                format!("param '{name}': type enum but no one_of choices declared"),
            ));
        }
    }

    // Validate profiles against the declared params.
    if let Some(profiles) = wb.frontmatter.profiles.as_ref() {
        for (pname, block) in profiles {
            for (key, val) in block {
                match params.get(key) {
                    None => out.push(Diagnostic::warning(
                        "wb-param-002",
                        path,
                        format!("profile '{pname}': '{key}' is not a declared parameter"),
                    )),
                    Some(spec) => {
                        if let Some(v) = scalar_str(val) {
                            check_param_value(
                                key,
                                &spec.to_def(),
                                &v,
                                &format!("profile '{pname}'"),
                                "wb-param-002",
                                path,
                                out,
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Shared value-vs-declaration check used for both defaults and profile values.
#[allow(clippy::too_many_arguments)]
fn check_param_value(
    name: &str,
    def: &crate::params::ParamDef,
    value: &str,
    origin: &str,
    code: diagnostic::Code,
    path: &Path,
    out: &mut Vec<Diagnostic>,
) {
    if !def.one_of.is_empty() {
        let allowed: Vec<String> = def.one_of.iter().filter_map(scalar_str).collect();
        if !allowed.iter().any(|a| a == value) {
            out.push(Diagnostic::error(
                code,
                path,
                format!(
                    "param '{name}': {origin} value '{value}' is not one of [{}]",
                    allowed.join(", ")
                ),
            ));
            return;
        }
    }
    match def.type_.as_deref().unwrap_or("string") {
        "int" => {
            if value.parse::<i64>().is_err() {
                out.push(Diagnostic::error(
                    code,
                    path,
                    format!("param '{name}': {origin} value '{value}' is not a valid int"),
                ));
            }
        }
        "bool"
            if !matches!(
                value.to_ascii_lowercase().as_str(),
                "true" | "false" | "1" | "0" | "yes" | "no" | "on" | "off"
            ) =>
        {
            out.push(Diagnostic::error(
                code,
                path,
                format!("param '{name}': {origin} value '{value}' is not a valid bool"),
            ));
        }
        _ => {}
    }
}

/// wb-expect-001: malformed assertion line in an `expect` / `assert` fence.
fn check_expects(wb: &Workbook, path: &Path, out: &mut Vec<Diagnostic>) {
    for section in &wb.sections {
        if let Section::Expect(spec) = section {
            for err in &spec.errors {
                out.push(
                    Diagnostic::error(
                        "wb-expect-001",
                        path,
                        format!("expect fence at L{}: {err}", spec.line_number),
                    )
                    .with_span(Span::point(spec.line_number as u32, 1)),
                );
            }
        }
    }
}

fn check_workflow_nodes(wb: &Workbook, path: &Path, out: &mut Vec<Diagnostic>) {
    let Some(workflow) = wb.frontmatter.workflow.as_ref() else {
        return;
    };
    let workflow = &workflow.0;
    let Some(workflow_obj) = workflow.as_object() else {
        out.push(Diagnostic::error(
            "wb-workflow-002",
            path,
            "workflow must be a mapping/object",
        ));
        return;
    };
    let Some(nodes_value) = workflow_obj.get("nodes") else {
        return;
    };
    let Some(nodes) = nodes_value.as_object() else {
        out.push(Diagnostic::error(
            "wb-workflow-002",
            path,
            "workflow.nodes must be a mapping/object keyed by step id",
        ));
        return;
    };

    if nodes.is_empty() {
        return;
    }
    let mut step_ids: std::collections::BTreeSet<String> =
        wb.build_steps().into_iter().map(|s| s.id.0).collect();
    for section in &wb.sections {
        if let crate::parser::Section::Wait(spec) = section {
            if let Some(id) = spec.attrs.explicit_id.as_ref() {
                step_ids.insert(id.clone());
            }
        }
    }

    for (id, node) in nodes {
        let Some(node_obj) = node.as_object() else {
            out.push(Diagnostic::error(
                "wb-workflow-002",
                path,
                format!("workflow.nodes.{id} must be a mapping/object"),
            ));
            continue;
        };
        let primitive_ok = node_obj
            .get("primitive")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.trim().is_empty());
        if !primitive_ok {
            out.push(
                Diagnostic::warning(
                    "wb-workflow-003",
                    path,
                    format!("workflow.nodes.{id}.primitive should be a non-empty string"),
                )
                .with_help("callbacks compact each workflow node to id, primitive, and title"),
            );
        }
        if !step_ids.contains(id.as_str()) {
            out.push(
                Diagnostic::warning(
                    "wb-workflow-001",
                    path,
                    format!("workflow.nodes.{id} has no matching step or wait id"),
                )
                .with_help(format!(
                    "add `{{#{id}}}` to the matching code/browser/wait fence"
                )),
            );
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
    fn duplicate_explicit_step_id_emits_wb_step_001() {
        let content = "```bash {#login}\necho one\n```\n\n```bash {#login}\necho two\n```\n";
        let diags = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags.iter().any(|d| d.code == "wb-step-001"),
            "expected wb-step-001, got: {diags:?}"
        );
    }

    #[test]
    fn unique_step_ids_no_wb_step_001() {
        let content = "```bash {#login}\necho one\n```\n\n```bash {#deploy}\necho two\n```\n";
        let diags = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            !diags.iter().any(|d| d.code == "wb-step-001"),
            "should not emit wb-step-001 for unique ids: {diags:?}"
        );
    }

    #[test]
    fn duplicate_wait_and_step_id_emits_wb_step_001() {
        let content =
            "```wait {#approval}\nkind: manual\nbind: ok\n```\n\n```bash {#approval}\necho ok\n```\n";
        let diags = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags.iter().any(|d| d.code == "wb-step-001"),
            "expected wb-step-001, got: {diags:?}"
        );
    }

    #[test]
    fn workflow_nodes_without_matching_step_warn() {
        let diags = validate_content(
            r#"---
workflow:
  slug: demo
  nodes:
    balance:
      primitive: stripe/balance
    missing:
      primitive: drive/upload
---
```bash {#balance}
echo ok
```
"#,
            Path::new("wf.md"),
            &ValidateOptions { strict: false },
        );
        let workflow_warnings: Vec<_> = diags
            .iter()
            .filter(|d| d.code == "wb-workflow-001")
            .collect();
        assert_eq!(workflow_warnings.len(), 1, "got: {diags:?}");
        assert!(workflow_warnings[0].message.contains("missing"));
    }

    #[test]
    fn workflow_nodes_can_match_wait_id() {
        let diags = validate_content(
            r#"---
workflow:
  slug: demo
  nodes:
    approval:
      primitive: wait/manual-approval
---
```wait {#approval}
kind: manual
bind: approved
```
"#,
            Path::new("wf.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            !diags.iter().any(|d| d.code == "wb-workflow-001"),
            "wait ids should satisfy workflow node declarations, got: {diags:?}"
        );
    }

    #[test]
    fn workflow_must_be_object() {
        let diags = validate_content(
            "---\nworkflow: not-a-map\n---\n\n```bash\necho ok\n```\n",
            Path::new("wf.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags.iter().any(|d| d.code == "wb-workflow-002"),
            "expected wb-workflow-002, got: {diags:?}"
        );
    }

    #[test]
    fn workflow_nodes_must_be_object() {
        let diags = validate_content(
            "---\nworkflow:\n  slug: demo\n  nodes: [bad]\n---\n\n```bash\necho ok\n```\n",
            Path::new("wf.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags.iter().any(|d| d.code == "wb-workflow-002"),
            "expected wb-workflow-002, got: {diags:?}"
        );
    }

    #[test]
    fn workflow_node_without_primitive_warns() {
        let diags = validate_content(
            r#"---
workflow:
  slug: demo
  nodes:
    export:
      title: Export
---
```bash {#export}
echo ok
```
"#,
            Path::new("wf.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == "wb-workflow-003" && d.severity == Severity::Warning),
            "expected wb-workflow-003 warning, got: {diags:?}"
        );
    }

    #[test]
    fn fence_attr_shadows_legacy_emits_wb_step_002() {
        // Block 1 has a legacy `timeouts: {1: 30s}` AND a fence attr `timeout=2m`.
        // The fence attr wins; we warn so the user can drop the legacy entry.
        let content = "---\ntimeouts:\n  1: 30s\n---\n\n```bash {timeout=2m}\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags.iter().any(|d| d.code == "wb-step-002"),
            "expected wb-step-002, got: {diags:?}"
        );
    }

    #[test]
    fn fence_attr_alone_does_not_emit_wb_step_002() {
        // Just the fence attr, no legacy entry — no shadowing, no warning.
        let content = "```bash {timeout=2m}\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            !diags.iter().any(|d| d.code == "wb-step-002"),
            "should not emit wb-step-002 when no legacy shadowing: {diags:?}"
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

    #[test]
    fn unknown_fence_kv_attr_warns() {
        let content = "```bash {flavor=spicy}\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: false },
        );
        let d = diags
            .iter()
            .find(|d| d.code == "wb-attr-001")
            .expect("expected wb-attr-001");
        assert_eq!(d.severity, Severity::Warning);
        assert!(d.message.contains("flavor"), "message: {}", d.message);
    }

    #[test]
    fn unknown_fence_flag_warns() {
        let content = "```bash {retryable}\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == "wb-attr-001" && d.message.contains("retryable")),
            "expected wb-attr-001 for unknown flag, got: {diags:?}"
        );
    }

    #[test]
    fn known_fence_attrs_do_not_warn() {
        let content =
            "```bash {#step1 .critical timeout=30s retries=2 continue_on_error}\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            !diags.iter().any(|d| d.code == "wb-attr-001"),
            "known attrs should not warn, got: {diags:?}"
        );
    }

    #[test]
    fn conditional_attrs_do_not_warn() {
        // when= / skip_if= are pulled into dedicated fields, not kv.
        let content = "```bash {when=$CI skip_if=$DRY_RUN}\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            !diags.iter().any(|d| d.code == "wb-attr-001"),
            "when/skip_if should not warn, got: {diags:?}"
        );
    }

    #[test]
    fn unknown_attr_promoted_to_error_in_strict() {
        let content = "```bash {flavor=spicy}\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("test.md"),
            &ValidateOptions { strict: true },
        );
        let d = diags
            .iter()
            .find(|d| d.code == "wb-attr-001")
            .expect("expected wb-attr-001");
        assert_eq!(d.severity, Severity::Error);
    }

    // ── validate_dir ──────────────────────────────────────────────────────

    #[test]
    fn validate_dir_walks_md_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("clean.md"),
            "---\ntitle: OK\n---\n\n```bash\necho ok\n```\n",
        )
        .unwrap();
        // A file with a bad secret provider → guaranteed error diagnostic.
        std::fs::write(
            dir.path().join("bad.md"),
            "---\nsecrets:\n  provider: nope\n---\n",
        )
        .unwrap();
        // A non-markdown file should be ignored.
        std::fs::write(dir.path().join("notes.txt"), "ignored").unwrap();
        // A .markdown extension should be included.
        std::fs::write(dir.path().join("also.markdown"), "```bash\necho hi\n```\n").unwrap();

        let diags = validate_dir(dir.path(), &ValidateOptions { strict: false });
        assert!(
            diags.iter().any(|d| d.code == "wb-secret-001"),
            "expected the bad.md secret error, got: {diags:?}"
        );
    }

    #[test]
    fn validate_dir_missing_returns_error() {
        let diags = validate_dir(
            Path::new("/nonexistent/dir/xyz"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags.iter().any(|d| d.code == "wb-yaml-001"),
            "expected a read error, got: {diags:?}"
        );
    }

    // ── timeouts._default / retries / continue_on_error ranges ─────────────

    #[test]
    fn bad_default_duration_emits_wb_fm_003() {
        let content = "---\ntimeouts:\n  _default: 5bananas\n---\n\n```bash\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("t.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == "wb-fm-003" && d.message.contains("_default")),
            "expected wb-fm-003 for _default, got: {diags:?}"
        );
    }

    #[test]
    fn retries_out_of_range_emits_wb_fm_006() {
        let content = "---\nretries:\n  9: 2\n---\n\n```bash\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("t.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == "wb-fm-006" && d.message.contains("retries")),
            "expected wb-fm-006 for retries, got: {diags:?}"
        );
    }

    #[test]
    fn continue_on_error_out_of_range_emits_wb_fm_006() {
        let content = "---\ncontinue_on_error: [9]\n---\n\n```bash\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("t.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == "wb-fm-006" && d.message.contains("continue_on_error")),
            "expected wb-fm-006 for continue_on_error, got: {diags:?}"
        );
    }

    // ── include resolution ─────────────────────────────────────────────────

    #[test]
    fn required_missing_target_emits_wb_inc_001() {
        let content = "---\nrequired:\n  - ./nope.md\n---\n\n```bash\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("t.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == "wb-inc-001" && d.message.contains("required")),
            "expected wb-inc-001 for required, got: {diags:?}"
        );
    }

    #[test]
    fn circular_include_emits_wb_inc_002() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.md"), "```include\npath: ./b.md\n```\n").unwrap();
        std::fs::write(dir.path().join("b.md"), "```include\npath: ./a.md\n```\n").unwrap();
        let diags = validate_file(&dir.path().join("a.md"), &ValidateOptions { strict: false });
        assert!(
            diags.iter().any(|d| d.code == "wb-inc-002"),
            "expected wb-inc-002 for circular include, got: {diags:?}"
        );
    }

    #[test]
    fn unreadable_include_target_emits_wb_inc_003() {
        let dir = tempfile::tempdir().unwrap();
        // An include whose target is a directory: exists (so no wb-inc-001) but
        // cannot be read as a file → wb-inc-003.
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("main.md"), "```include\npath: ./sub\n```\n").unwrap();
        let diags = validate_file(
            &dir.path().join("main.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags.iter().any(|d| d.code == "wb-inc-003"),
            "expected wb-inc-003 for unreadable target, got: {diags:?}"
        );
    }

    // ── secret providers ───────────────────────────────────────────────────

    #[test]
    fn multiple_secret_providers_flag_unknown() {
        let content =
            "---\nsecrets:\n  - provider: env\n  - provider: bogus\n---\n\n```bash\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("t.md"),
            &ValidateOptions { strict: false },
        );
        let secret: Vec<_> = diags.iter().filter(|d| d.code == "wb-secret-001").collect();
        assert_eq!(
            secret.len(),
            1,
            "expected one unknown provider, got: {diags:?}"
        );
        assert!(secret[0].message.contains("bogus"));
    }

    // ── params (wb-param-001 / wb-param-002) ───────────────────────────────

    #[test]
    fn param_unknown_type_emits_wb_param_001() {
        let content = "---\nparams:\n  x:\n    type: blob\n---\n\n```bash\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("t.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == "wb-param-001" && d.message.contains("unknown type")),
            "expected wb-param-001 unknown type, got: {diags:?}"
        );
    }

    #[test]
    fn param_default_not_in_one_of_emits_wb_param_001() {
        let content = "---\nparams:\n  region:\n    type: enum\n    one_of: [a, b]\n    default: c\n---\n\n```bash\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("t.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == "wb-param-001" && d.message.contains("not one of")),
            "expected wb-param-001 one_of violation, got: {diags:?}"
        );
    }

    #[test]
    fn param_default_bad_int_emits_wb_param_001() {
        let content = "---\nparams:\n  n:\n    type: int\n    default: notnum\n---\n\n```bash\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("t.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == "wb-param-001" && d.message.contains("valid int")),
            "expected wb-param-001 int violation, got: {diags:?}"
        );
    }

    #[test]
    fn param_default_bad_bool_emits_wb_param_001() {
        let content = "---\nparams:\n  flag:\n    type: bool\n    default: maybe\n---\n\n```bash\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("t.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == "wb-param-001" && d.message.contains("valid bool")),
            "expected wb-param-001 bool violation, got: {diags:?}"
        );
    }

    #[test]
    fn enum_without_one_of_warns_wb_param_001() {
        let content = "---\nparams:\n  region:\n    type: enum\n---\n\n```bash\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("t.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags.iter().any(|d| d.code == "wb-param-001"
                && d.severity == Severity::Warning
                && d.message.contains("no one_of")),
            "expected wb-param-001 warning, got: {diags:?}"
        );
    }

    #[test]
    fn valid_params_no_diags() {
        let content = "---\nparams:\n  region:\n    type: enum\n    one_of: [us, eu]\n    default: us\n  replicas:\n    type: int\n    default: 2\n  dry:\n    type: bool\n    default: true\n  service: api\n---\n\n```bash\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("t.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            !diags.iter().any(|d| d.code.starts_with("wb-param")),
            "expected no param diags, got: {diags:?}"
        );
    }

    #[test]
    fn profile_without_params_warns_wb_param_002() {
        let content = "---\nprofiles:\n  prod:\n    region: eu\n---\n\n```bash\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("t.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == "wb-param-002" && d.message.contains("no params are declared")),
            "expected wb-param-002 (no params), got: {diags:?}"
        );
    }

    #[test]
    fn profile_undeclared_key_warns_wb_param_002() {
        let content = "---\nparams:\n  region:\n    type: string\n    default: us\nprofiles:\n  prod:\n    bogus: x\n---\n\n```bash\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("t.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags.iter().any(|d| d.code == "wb-param-002"
                && d.message.contains("not a declared parameter")),
            "expected wb-param-002 (undeclared), got: {diags:?}"
        );
    }

    #[test]
    fn profile_value_violates_type_emits_wb_param_002() {
        let content = "---\nparams:\n  replicas:\n    type: int\n    default: 1\nprofiles:\n  prod:\n    replicas: lots\n---\n\n```bash\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("t.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags.iter().any(|d| d.code == "wb-param-002"
                && d.message.contains("valid int")
                && d.message.contains("prod")),
            "expected wb-param-002 profile type violation, got: {diags:?}"
        );
    }

    // ── expect fences (wb-expect-001) ──────────────────────────────────────

    #[test]
    fn malformed_expect_emits_wb_expect_001() {
        let content =
            "```bash\necho hi\n```\n\n```expect\nthis is not a valid assertion line\n```\n";
        let diags = validate_content(
            content,
            Path::new("t.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags.iter().any(|d| d.code == "wb-expect-001"),
            "expected wb-expect-001, got: {diags:?}"
        );
    }

    // ── wb-step-002 with a non-timeout field (field_to_legacy_key default) ──

    #[test]
    fn retries_fence_attr_shadows_legacy_wb_step_002() {
        let content = "---\nretries:\n  1: 2\n---\n\n```bash {retries=3}\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("t.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == "wb-step-002" && d.message.contains("retries")),
            "expected wb-step-002 for retries shadowing, got: {diags:?}"
        );
    }

    // ── workflow node shape checks ─────────────────────────────────────────

    #[test]
    fn workflow_node_not_object_emits_wb_workflow_002() {
        let content = r#"---
workflow:
  slug: demo
  nodes:
    bad: just-a-string
---
```bash {#bad}
echo hi
```
"#;
        let diags = validate_content(
            content,
            Path::new("wf.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == "wb-workflow-002" && d.message.contains("bad")),
            "expected wb-workflow-002 for non-object node, got: {diags:?}"
        );
    }

    #[test]
    fn workflow_without_nodes_is_clean() {
        // workflow object present but no `nodes` key → early return, no diags.
        let content = "---\nworkflow:\n  slug: demo\n---\n\n```bash\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("wf.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            !diags.iter().any(|d| d.code.starts_with("wb-workflow")),
            "expected no workflow diags, got: {diags:?}"
        );
    }

    #[test]
    fn workflow_empty_nodes_is_clean() {
        let content = "---\nworkflow:\n  slug: demo\n  nodes: {}\n---\n\n```bash\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("wf.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            !diags.iter().any(|d| d.code.starts_with("wb-workflow")),
            "expected no workflow diags for empty nodes, got: {diags:?}"
        );
    }

    // ── extract_yaml_region: opening `---` not followed by a newline ───────

    #[test]
    fn frontmatter_without_newline_after_open_dashes() {
        // "---title: x" — the opening dashes are immediately followed by content,
        // exercising the no-leading-newline branch of extract_yaml_region.
        let content = "---title: x\n---\n\n```bash\necho hi\n```\n";
        let diags = validate_content(
            content,
            Path::new("t.md"),
            &ValidateOptions { strict: false },
        );
        // Should not panic; region is extracted and parsed. (Behavior asserted
        // loosely — the point is the code path executes.)
        let _ = diags;
    }

    // ── yaml region: a deserialize error that may lack a location ───────────

    #[test]
    fn frontmatter_type_error_emits_wb_yaml_001() {
        // `secrets: 123` fails the tolerant Frontmatter parse (wrong shape).
        let content = "---\nsecrets: 123\n---\n";
        let diags = validate_content(
            content,
            Path::new("t.md"),
            &ValidateOptions { strict: false },
        );
        assert!(
            diags.iter().any(|d| d.code == "wb-yaml-001"),
            "expected wb-yaml-001 for type error, got: {diags:?}"
        );
    }

    // ── exit_code_for mapping ──────────────────────────────────────────────

    #[test]
    fn exit_code_for_reflects_severity_and_strict() {
        let clean: Vec<Diagnostic> = vec![];
        assert_eq!(exit_code_for(&clean, false), exit_codes::EXIT_SUCCESS);

        let warn = vec![Diagnostic::warning("wb-fm-006", Path::new("t.md"), "w")];
        assert_eq!(exit_code_for(&warn, false), exit_codes::EXIT_SUCCESS);
        assert_eq!(
            exit_code_for(&warn, true),
            exit_codes::EXIT_WORKBOOK_INVALID
        );

        let err = vec![Diagnostic::error("wb-secret-001", Path::new("t.md"), "e")];
        assert_eq!(
            exit_code_for(&err, false),
            exit_codes::EXIT_WORKBOOK_INVALID
        );
    }

    // ── check_fence_yaml: browser / include / wait sections traversed ──────

    #[test]
    fn fence_yaml_traverses_browser_include_wait_sections() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("inc.md"), "```bash\necho inc\n```\n").unwrap();
        let main = format!(
            "{}{}{}",
            "```browser\nverbs:\n  - goto: https://example.com\n```\n\n",
            "```include\npath: ./inc.md\n```\n\n",
            "```wait\nkind: manual\nbind: ok\n```\n",
        );
        std::fs::write(dir.path().join("main.md"), &main).unwrap();
        let diags = validate_file(
            &dir.path().join("main.md"),
            &ValidateOptions { strict: false },
        );
        // include target exists, so no inc-001; the point is no panic across
        // the browser/include/wait section arms.
        assert!(
            !diags.iter().any(|d| d.code == "wb-inc-001"),
            "include resolves; got: {diags:?}"
        );
    }
}
