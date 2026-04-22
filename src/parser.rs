use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Frontmatter {
    pub title: Option<String>,
    pub runtime: Option<String>,
    pub venv: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub vars: Option<HashMap<String, String>>,
    pub redact: Option<Vec<String>>,
    pub secrets: Option<SecretsConfig>,
    pub setup: Option<SetupConfig>,
    pub exec: Option<ExecConfig>,
    pub working_dir: Option<DirConfig>,
    pub requires: Option<RequiresConfig>,
    /// Per-block timeout map, keyed by 1-based block number. Values are
    /// duration strings ("30s", "5m", "2h") — bare integers are seconds.
    /// Missing keys fall back to the 300s default.
    pub timeouts: Option<HashMap<u32, String>>,
    /// Per-block retry count, keyed by 1-based block number. Value is the
    /// number of extra attempts after the first failure; `0`/missing = no
    /// retry. Retries run with a 500ms delay between attempts. If a retry
    /// is triggered by a timeout (which kills the session child), later
    /// attempts will execute in a fresh language session.
    pub retries: Option<HashMap<u32, u32>>,
    /// Block numbers (1-based) whose failure should not halt a `--bail`
    /// run. The block's failure is still recorded and emitted via
    /// callbacks; execution just continues to the next block.
    pub continue_on_error: Option<Vec<u32>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RequiresConfig {
    /// Sandbox type: "python", "node", or "custom"
    pub sandbox: String,
    /// System packages to install via apt-get
    #[serde(default)]
    pub apt: Vec<String>,
    /// Python packages to install via uv pip
    #[serde(default)]
    pub pip: Vec<String>,
    /// Node packages to install via npm
    #[serde(default)]
    pub node: Vec<String>,
    /// Path to a custom Dockerfile (only for sandbox: custom)
    pub dockerfile: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum DirConfig {
    /// Global: `working_dir: /path/to/dir`
    Global(String),
    /// Per-language: `working_dir: { python: ./src, bash: /tmp }`
    PerLanguage(HashMap<String, String>),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum ExecConfig {
    /// Global prefix: `exec: "docker exec mycontainer"`
    Global(String),
    /// Per-language: `exec: { python: "uv run", node: "pnpm exec" }`
    PerLanguage(HashMap<String, String>),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum StringOrVec {
    Single(String),
    Multiple(Vec<String>),
}

impl StringOrVec {
    pub fn as_vec(&self) -> Vec<&str> {
        match self {
            StringOrVec::Single(s) => vec![s.as_str()],
            StringOrVec::Multiple(v) => v.iter().map(|s| s.as_str()).collect(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum SetupConfig {
    Single(String),
    Multiple(Vec<String>),
    Structured {
        run: StringOrVec,
        dir: Option<String>,
    },
}

impl SetupConfig {
    pub fn commands(&self) -> Vec<&str> {
        match self {
            SetupConfig::Single(cmd) => vec![cmd.as_str()],
            SetupConfig::Multiple(cmds) => cmds.iter().map(|s| s.as_str()).collect(),
            SetupConfig::Structured { run, .. } => run.as_vec(),
        }
    }

    pub fn dir(&self) -> Option<&str> {
        match self {
            SetupConfig::Structured { dir, .. } => dir.as_deref(),
            _ => None,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum SecretsConfig {
    Single(SecretProvider),
    Multiple(Vec<SecretProvider>),
}

#[derive(Debug, Deserialize, Clone)]
pub struct SecretProvider {
    pub provider: String,
    /// For doppler: project name. For yard: unused.
    pub project: Option<String>,
    /// For yard/custom: the shell command to run
    pub command: Option<String>,
    /// For env: specific keys to pull
    pub keys: Option<Vec<String>>,
}

#[derive(Debug)]
pub struct CodeBlock {
    pub language: String,
    pub code: String,
    pub line_number: usize,
    /// `{no-run}` info-string flag: parse/render like a normal block but never execute.
    pub skip_execution: bool,
    /// `{silent}` info-string flag: execute normally but suppress step.complete/step.failed callbacks.
    pub silent: bool,
    /// `{when=EXPR}` info-string attribute: runtime-conditional execution. The
    /// block runs only if EXPR evaluates true against the session env. Evaluated
    /// per-run; unlike `{no-run}` the decision can differ between runs of the
    /// same workbook as env changes. `None` = always run.
    pub when: Option<String>,
    /// `{skip_if=EXPR}` info-string attribute: inverse of `when`. Block is skipped
    /// if EXPR evaluates true. Composes with `when` via AND — a block runs when
    /// `when` is true *and* `skip_if` is false.
    pub skip_if: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum BindSpec {
    /// Single variable name: `bind: otp_code`
    Single(String),
    /// Multiple variable names: `bind: [otp_code, sender]`
    Multiple(Vec<String>),
}

impl BindSpec {
    #[allow(dead_code)]
    pub fn names(&self) -> Vec<&str> {
        match self {
            BindSpec::Single(s) => vec![s.as_str()],
            BindSpec::Multiple(v) => v.iter().map(|s| s.as_str()).collect(),
        }
    }
}

/// Bind names that would shadow an env var the shell or `wb` itself relies on.
/// Resolved signal values get exported into the child process env on resume,
/// so binding to one of these would silently break the workbook — or worse,
/// the runtime (e.g. `bind: PATH` replaces the executable search path).
const RESERVED_BIND_NAMES: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "SHELL",
    "PWD",
    "OLDPWD",
    "LD_LIBRARY_PATH",
    "DYLD_LIBRARY_PATH",
    "DYLD_FALLBACK_LIBRARY_PATH",
    "TRIGGER_RUN_ID",
    "TMPDIR",
    "LANG",
    "LC_ALL",
];

/// Returns the first reserved name hit, if any. Names starting with `WB_`
/// are reserved as a namespace for wb-internal variables.
pub fn reserved_bind_name<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    for name in names {
        if name.starts_with("WB_") {
            return Some(name);
        }
        if RESERVED_BIND_NAMES
            .iter()
            .any(|r| r.eq_ignore_ascii_case(name))
        {
            return Some(name);
        }
    }
    None
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct WaitSpec {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default, rename = "match")]
    pub match_: Option<serde_yaml::Value>,
    #[serde(default)]
    pub bind: Option<BindSpec>,
    #[serde(default)]
    pub timeout: Option<String>,
    #[serde(default)]
    pub on_timeout: Option<String>,
    #[serde(skip, default)]
    pub line_number: usize,
    #[serde(skip, default)]
    pub section_index: usize,
}

/// Browser slice — body parsed into a structured envelope (`session`, `on_pause`)
/// with an opaque `verbs` list forwarded verbatim to the sidecar. Verb vocabulary
/// is sidecar-defined; `wb` does not interpret individual verbs.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct BrowserSliceSpec {
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub on_pause: Option<String>,
    /// Opaque provider-side profile identifier (e.g. browser-use `profileId`).
    /// Hard-coded per runbook by whatever emits it (UI editor, codegen) so
    /// rotating the bound auth state is a workbook re-emit, not an env-var
    /// shuffle. Providers that don't support profiles log + ignore.
    #[serde(default, rename = "profile_id")]
    pub profile: Option<String>,
    #[serde(default)]
    pub verbs: Vec<serde_yaml::Value>,
    #[serde(skip, default)]
    pub line_number: usize,
    #[serde(skip, default)]
    pub section_index: usize,
    #[serde(skip, default)]
    pub raw: String,
    #[serde(skip, default)]
    pub skip_execution: bool,
    #[serde(skip, default)]
    pub silent: bool,
    #[serde(skip, default)]
    pub when: Option<String>,
    #[serde(skip, default)]
    pub skip_if: Option<String>,
}

/// Workbook-composition fence (```include ```). Body is YAML with a `path:`
/// pointing at another workbook. Resolved away before execution via
/// `resolve_includes` — downstream code never sees `Section::Include`.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct IncludeSpec {
    pub path: String,
    #[serde(skip, default)]
    pub line_number: usize,
    #[serde(skip, default)]
    pub section_index: usize,
}

#[derive(Debug)]
pub enum Section {
    Text(String),
    Code(CodeBlock),
    Wait(WaitSpec),
    Browser(BrowserSliceSpec),
    Include(IncludeSpec),
}

#[derive(Debug)]
pub struct Workbook {
    pub frontmatter: Frontmatter,
    pub sections: Vec<Section>,
}

impl Workbook {
    /// Count of executable units (code blocks + browser slices).
    /// Browser slices consume a block index and show up in progress/callbacks
    /// exactly like code blocks do. Blocks flagged `{no-run}` are excluded —
    /// they're parsed but never execute, so they don't count toward progress.
    pub fn code_block_count(&self) -> usize {
        self.sections
            .iter()
            .filter(|s| match s {
                Section::Code(b) => !b.skip_execution,
                Section::Browser(b) => !b.skip_execution,
                _ => false,
            })
            .count()
    }
}

pub fn parse(input: &str) -> Workbook {
    let (frontmatter, body) = extract_frontmatter(input);
    let sections = extract_sections(&body);

    Workbook {
        frontmatter,
        sections,
    }
}

/// Expand any ```include``` fences by splicing the target workbook's sections
/// into place. Target frontmatter is intentionally ignored — the parent
/// workbook's runtime/secrets/env/venv control the run. Target paths resolve
/// relative to the directory of the including workbook, not the CWD.
///
/// Cycle detection tracks canonical paths of ancestors currently being
/// resolved, so `A → B → A` is caught but `A → B, A → B` (same target included
/// twice at different positions) is allowed.
pub fn resolve_includes(wb: Workbook, parent_path: &Path) -> Result<Workbook, String> {
    let parent_canonical = parent_path.canonicalize().map_err(|e| {
        format!(
            "cannot resolve workbook path {}: {}",
            parent_path.display(),
            e
        )
    })?;
    let mut visiting = HashSet::new();
    visiting.insert(parent_canonical.clone());
    let base_dir = parent_canonical
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let resolved = resolve_sections(wb.sections, &base_dir, &mut visiting)?;
    Ok(Workbook {
        frontmatter: wb.frontmatter,
        sections: resolved,
    })
}

fn resolve_sections(
    sections: Vec<Section>,
    base_dir: &Path,
    visiting: &mut HashSet<PathBuf>,
) -> Result<Vec<Section>, String> {
    let mut out = Vec::new();
    for section in sections {
        match section {
            Section::Include(spec) => {
                let target = base_dir.join(&spec.path);
                let target_canonical = target.canonicalize().map_err(|e| {
                    format!(
                        "include at L{}: cannot resolve path '{}' (relative to {}): {}",
                        spec.line_number,
                        spec.path,
                        base_dir.display(),
                        e
                    )
                })?;
                if visiting.contains(&target_canonical) {
                    return Err(format!(
                        "include at L{}: circular include of '{}' (already being resolved)",
                        spec.line_number,
                        target_canonical.display()
                    ));
                }
                let content = fs::read_to_string(&target_canonical).map_err(|e| {
                    format!(
                        "include at L{}: cannot read '{}': {}",
                        spec.line_number,
                        target_canonical.display(),
                        e
                    )
                })?;
                let inner = parse(&content);
                let inner_base = target_canonical
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));
                visiting.insert(target_canonical.clone());
                let inner_resolved = resolve_sections(inner.sections, &inner_base, visiting)?;
                visiting.remove(&target_canonical);
                out.extend(inner_resolved);
            }
            other => out.push(other),
        }
    }
    Ok(out)
}

fn extract_frontmatter(input: &str) -> (Frontmatter, String) {
    let trimmed = input.trim_start();
    if !trimmed.starts_with("---") {
        return (Frontmatter::default(), input.to_string());
    }

    // Find closing ---
    let after_opening = &trimmed[3..];
    let close_pos = after_opening.find("\n---");
    match close_pos {
        Some(pos) => {
            let yaml_str = &after_opening[..pos];
            let rest = &after_opening[pos + 4..]; // skip \n---
                                                  // Skip the newline after closing ---
            let rest = rest.strip_prefix('\n').unwrap_or(rest);

            let frontmatter: Frontmatter = match serde_yaml::from_str(yaml_str) {
                Ok(fm) => fm,
                Err(e) => {
                    eprintln!("wb: frontmatter parse warning: {}", e);
                    Frontmatter::default()
                }
            };
            (frontmatter, rest.to_string())
        }
        None => (Frontmatter::default(), input.to_string()),
    }
}

/// Parsed info string: language token + optional `{no-run, silent, when=, skip_if=, ...}` attrs.
#[derive(Debug, Default, PartialEq, Eq)]
struct InfoString {
    language: String,
    skip_execution: bool,
    silent: bool,
    /// `when=EXPR` — run only if EXPR is truthy at runtime.
    when: Option<String>,
    /// `skip_if=EXPR` — skip if EXPR is truthy at runtime.
    skip_if: Option<String>,
}

/// Split a fence info string like `bash {no-run, silent, when=$X}` into language + attrs.
///
/// Brace cluster is optional and can appear anywhere after the language token. Attrs
/// inside braces are comma or whitespace separated; unknown attrs are currently
/// ignored so the parser stays forward-compatible. Key/value attrs (`when=`, `skip_if=`)
/// must have no whitespace in the value — the split would fracture the expression.
fn parse_info_string(info: &str) -> InfoString {
    let info = info.trim();
    let (lang_part, flag_part) = match (info.find('{'), info.rfind('}')) {
        (Some(open), Some(close)) if close > open => {
            (info[..open].trim(), Some(&info[open + 1..close]))
        }
        _ => (info, None),
    };
    let mut out = InfoString {
        language: lang_part.to_string(),
        ..Default::default()
    };
    if let Some(flags) = flag_part {
        for flag in flags
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if let Some(expr) = flag.strip_prefix("when=") {
                out.when = Some(expr.to_string());
            } else if let Some(expr) = flag.strip_prefix("skip_if=") {
                out.skip_if = Some(expr.to_string());
            } else {
                match flag {
                    "no-run" => out.skip_execution = true,
                    "silent" => out.silent = true,
                    _ => {}
                }
            }
        }
    }
    out
}

/// Evaluate a `when=` / `skip_if=` expression against a resolved env map.
///
/// Grammar (intentionally tiny):
///   - `$VAR`          → truthy check on env[VAR]: non-empty, and not "0"/"false"/"no"/"off" (case-insensitive)
///   - `$VAR=value`    → env[VAR] equals value (missing var ≠ value → false)
///   - `$VAR!=value`   → env[VAR] does not equal value (missing var counts as not-equal → true)
///   - `!<expr>`       → boolean NOT of any of the above
///
/// No shell, no arithmetic, no Jinja. If the expression is malformed, a warning is
/// logged and `false` is returned — fail-safe: a broken conditional does not run.
pub fn evaluate_condition(expr: &str, env: &HashMap<String, String>) -> bool {
    let expr = expr.trim();
    let (negate, expr) = match expr.strip_prefix('!') {
        Some(rest) => (true, rest.trim()),
        None => (false, expr),
    };
    let result = match expr.strip_prefix('$') {
        Some(var) => {
            if let Some((name, value)) = var.split_once("!=") {
                env.get(name).map(|v| v != value).unwrap_or(true)
            } else if let Some((name, value)) = var.split_once('=') {
                env.get(name).map(|v| v == value).unwrap_or(false)
            } else {
                is_truthy(env.get(var))
            }
        }
        None => {
            eprintln!(
                "wb: invalid when/skip_if expression '{}' — must start with '$' (or '!$'). Treating as false.",
                expr
            );
            false
        }
    };
    if negate { !result } else { result }
}

fn is_truthy(v: Option<&String>) -> bool {
    match v {
        None => false,
        Some(s) => {
            let t = s.trim();
            if t.is_empty() {
                return false;
            }
            !matches!(t.to_ascii_lowercase().as_str(), "0" | "false" | "no" | "off")
        }
    }
}

/// Build the env map used to evaluate `when=` / `skip_if=` expressions.
///
/// Merges the process env (inherited by child subprocesses by default) with
/// the session env (frontmatter + resolved secrets + WB_* internals), with
/// session values winning on conflict. The resulting map matches what a bash
/// block would see at runtime, so `skip_if=$CI` behaves the way an author
/// expects when `CI=1` is set in the parent shell.
pub fn resolved_env(session_env: &HashMap<String, String>) -> HashMap<String, String> {
    let mut merged: HashMap<String, String> = std::env::vars().collect();
    for (k, v) in session_env {
        merged.insert(k.clone(), v.clone());
    }
    merged
}

/// Decide whether a block should be skipped this run. Returns a human-readable
/// reason (matches the hint printed in the run loop) or `None` to run normally.
pub fn should_skip_block(
    when: Option<&str>,
    skip_if: Option<&str>,
    env: &HashMap<String, String>,
) -> Option<String> {
    if let Some(expr) = when {
        if !evaluate_condition(expr, env) {
            return Some(format!("when={} → false", expr));
        }
    }
    if let Some(expr) = skip_if {
        if evaluate_condition(expr, env) {
            return Some(format!("skip_if={} → true", expr));
        }
    }
    None
}

fn extract_sections(body: &str) -> Vec<Section> {
    let mut sections = Vec::new();
    let mut current_text = String::new();
    let mut lines = body.lines().enumerate().peekable();

    while let Some((line_num, line)) = lines.next() {
        if line.starts_with("```") && line.len() > 3 {
            // Opening fence with language + optional `{flag, flag}` attribute cluster
            let info = parse_info_string(&line[3..]);
            let language = info.language.clone();

            // Wait fence: parse YAML body into a WaitSpec
            if language.eq_ignore_ascii_case("wait") {
                if !current_text.is_empty() {
                    sections.push(Section::Text(current_text.clone()));
                    current_text.clear();
                }
                let mut body_lines = Vec::new();
                for (_ln, body_line) in lines.by_ref() {
                    if body_line.trim() == "```" {
                        break;
                    }
                    body_lines.push(body_line);
                }
                let yaml = body_lines.join("\n");
                let mut spec: WaitSpec = match serde_yaml::from_str(&yaml) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("wb: wait block parse error at L{}: {}", line_num + 1, e);
                        WaitSpec::default()
                    }
                };
                if let Some(ref bind) = spec.bind {
                    if let Some(reserved) = reserved_bind_name(bind.names()) {
                        eprintln!(
                            "wb: wait block at L{}: bind name '{}' is reserved (would override an env var the shell or wb depends on). \
                             Rename the bind (e.g. '{}_value') and update references in later blocks.",
                            line_num + 1,
                            reserved,
                            reserved.to_lowercase()
                        );
                        spec.bind = None;
                    }
                }
                spec.line_number = line_num + 1;
                spec.section_index = sections.len();
                sections.push(Section::Wait(spec));
                continue;
            }

            // Include fence: YAML body with `path:` pointing at another workbook.
            // Resolved away by `resolve_includes` before execution — execution-time
            // dispatch never sees these.
            if language.eq_ignore_ascii_case("include") {
                if !current_text.is_empty() {
                    sections.push(Section::Text(current_text.clone()));
                    current_text.clear();
                }
                let mut body_lines = Vec::new();
                for (_ln, body_line) in lines.by_ref() {
                    if body_line.trim() == "```" {
                        break;
                    }
                    body_lines.push(body_line);
                }
                let yaml = body_lines.join("\n");
                let mut spec: IncludeSpec = match serde_yaml::from_str(&yaml) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!(
                            "wb: include block parse error at L{}: {}",
                            line_num + 1,
                            e
                        );
                        IncludeSpec::default()
                    }
                };
                spec.line_number = line_num + 1;
                spec.section_index = sections.len();
                sections.push(Section::Include(spec));
                continue;
            }

            // Browser fence: parse YAML envelope, forward `verbs` opaquely to sidecar
            if language.eq_ignore_ascii_case("browser") {
                if !current_text.is_empty() {
                    sections.push(Section::Text(current_text.clone()));
                    current_text.clear();
                }
                let mut body_lines = Vec::new();
                for (_ln, body_line) in lines.by_ref() {
                    if body_line.trim() == "```" {
                        break;
                    }
                    body_lines.push(body_line);
                }
                let yaml = body_lines.join("\n");
                let mut spec: BrowserSliceSpec = match serde_yaml::from_str(&yaml) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("wb: browser block parse error at L{}: {}", line_num + 1, e);
                        BrowserSliceSpec::default()
                    }
                };
                spec.line_number = line_num + 1;
                spec.section_index = sections.len();
                spec.raw = yaml;
                spec.skip_execution = info.skip_execution;
                spec.silent = info.silent;
                spec.when = info.when.clone();
                spec.skip_if = info.skip_if.clone();
                sections.push(Section::Browser(spec));
                continue;
            }

            // Skip non-executable blocks (like yaml examples, json, etc that are just docs)
            // We execute: python, bash, sh, zsh, node, javascript, js, ruby, rb, perl, r
            if !is_executable_language(&language) {
                current_text.push_str(line);
                current_text.push('\n');
                // Consume until closing fence
                for (_ln, inner_line) in lines.by_ref() {
                    current_text.push_str(inner_line);
                    current_text.push('\n');
                    if inner_line.trim() == "```" {
                        break;
                    }
                }
                continue;
            }

            // Flush accumulated text
            if !current_text.is_empty() {
                sections.push(Section::Text(current_text.clone()));
                current_text.clear();
            }

            // Collect code lines until closing fence
            let mut code_lines = Vec::new();
            for (_ln, code_line) in lines.by_ref() {
                if code_line.trim() == "```" {
                    break;
                }
                code_lines.push(code_line);
            }

            sections.push(Section::Code(CodeBlock {
                language,
                code: code_lines.join("\n"),
                line_number: line_num + 1, // 1-indexed
                skip_execution: info.skip_execution,
                silent: info.silent,
                when: info.when,
                skip_if: info.skip_if,
            }));
        } else {
            current_text.push_str(line);
            current_text.push('\n');
        }
    }

    // Flush remaining text
    if !current_text.is_empty() {
        sections.push(Section::Text(current_text));
    }

    sections
}

/// Effective per-block policy resolved from `timeouts`, `retries`, and
/// `continue_on_error` frontmatter maps. `block_number` is 1-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockPolicy {
    /// Timeout for a single execution of this block. `None` = use the
    /// run-wide default (ExecutionContext::block_timeout).
    pub timeout_secs: Option<u64>,
    /// Retries *after* the first failure. `0` = run once.
    pub retries: u32,
    /// If true, a failure of this block does NOT trigger `--bail`.
    pub continue_on_error: bool,
}

impl Frontmatter {
    /// Resolve the per-block policy for a given 1-based block number.
    /// Unknown numbers return the all-default policy.
    pub fn block_policy(&self, block_number: u32) -> BlockPolicy {
        let timeout_secs = self
            .timeouts
            .as_ref()
            .and_then(|m| m.get(&block_number))
            .and_then(|s| match parse_duration_secs(s) {
                Ok(n) => Some(n),
                Err(e) => {
                    eprintln!(
                        "wb: ignoring invalid timeouts[{}]='{}': {}",
                        block_number, s, e
                    );
                    None
                }
            });
        let retries = self
            .retries
            .as_ref()
            .and_then(|m| m.get(&block_number))
            .copied()
            .unwrap_or(0);
        let continue_on_error = self
            .continue_on_error
            .as_ref()
            .map(|v| v.contains(&block_number))
            .unwrap_or(false);
        BlockPolicy {
            timeout_secs,
            retries,
            continue_on_error,
        }
    }
}

/// Parse durations like "30s", "5m", "2h", "1d" into seconds.
/// Bare integers are treated as seconds.
pub fn parse_duration_secs(s: &str) -> Result<u64, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err("empty duration".to_string());
    }
    let (num_part, unit) = match trimmed.chars().last() {
        Some(c) if c.is_ascii_alphabetic() => (&trimmed[..trimmed.len() - 1], c),
        _ => (trimmed, 's'),
    };
    let n: u64 = num_part
        .trim()
        .parse()
        .map_err(|_| format!("invalid duration '{}'", s))?;
    let mult = match unit.to_ascii_lowercase() {
        's' => 1,
        'm' => 60,
        'h' => 60 * 60,
        'd' => 60 * 60 * 24,
        _ => return Err(format!("unknown duration unit '{}'", unit)),
    };
    Ok(n * mult)
}

fn is_executable_language(lang: &str) -> bool {
    matches!(
        lang.to_lowercase().as_str(),
        "python"
            | "python3"
            | "py"
            | "bash"
            | "sh"
            | "zsh"
            | "shell"
            | "node"
            | "javascript"
            | "js"
            | "ruby"
            | "rb"
            | "perl"
            | "r"
            | "php"
            | "lua"
            | "swift"
            | "go"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_with_frontmatter() {
        let input = r#"---
title: Test Workbook
runtime: python
venv: ./.venv
secrets:
  provider: doppler
  project: my-project
---

# Hello

```python
print("hello")
```
"#;
        let wb = parse(input);
        assert_eq!(wb.frontmatter.title.as_deref(), Some("Test Workbook"));
        assert_eq!(wb.frontmatter.runtime.as_deref(), Some("python"));
        assert_eq!(wb.frontmatter.venv.as_deref(), Some("./.venv"));
        assert_eq!(wb.code_block_count(), 1);
    }

    #[test]
    fn test_parse_no_frontmatter() {
        let input = r#"# Just markdown

```bash
echo "hello"
```
"#;
        let wb = parse(input);
        assert!(wb.frontmatter.title.is_none());
        assert_eq!(wb.code_block_count(), 1);
    }

    #[test]
    fn test_parse_setup_single() {
        let input = r#"---
title: Setup Test
setup: uv sync
---

```bash
echo "hello"
```
"#;
        let wb = parse(input);
        let setup = wb.frontmatter.setup.unwrap();
        assert_eq!(setup.commands(), vec!["uv sync"]);
    }

    #[test]
    fn test_parse_setup_multiple() {
        let input = r#"---
title: Setup Test
setup:
  - uv sync
  - npm install
---

```bash
echo "hello"
```
"#;
        let wb = parse(input);
        let setup = wb.frontmatter.setup.unwrap();
        assert_eq!(setup.commands(), vec!["uv sync", "npm install"]);
    }

    #[test]
    fn test_parse_setup_structured() {
        let input = r#"---
setup:
  dir: ../../
  run:
    - uv sync
    - npm install
---

```bash
echo "hello"
```
"#;
        let wb = parse(input);
        let setup = wb.frontmatter.setup.unwrap();
        assert_eq!(setup.commands(), vec!["uv sync", "npm install"]);
        assert_eq!(setup.dir(), Some("../../"));
    }

    #[test]
    #[test]
    fn test_reserved_bind_name_exact() {
        assert_eq!(reserved_bind_name(std::iter::once("PATH")), Some("PATH"));
        assert_eq!(reserved_bind_name(std::iter::once("Home")), Some("Home"));
        assert_eq!(reserved_bind_name(std::iter::once("otp_code")), None);
    }

    #[test]
    fn test_reserved_bind_name_wb_prefix() {
        assert_eq!(
            reserved_bind_name(std::iter::once("WB_ARTIFACTS_DIR")),
            Some("WB_ARTIFACTS_DIR")
        );
        assert_eq!(
            reserved_bind_name(std::iter::once("WB_custom")),
            Some("WB_custom")
        );
        // Non-WB prefix is fine.
        assert_eq!(reserved_bind_name(std::iter::once("MY_VAR")), None);
    }

    #[test]
    fn test_reserved_bind_name_first_hit() {
        let names = vec!["otp_code", "PATH", "sender"];
        assert_eq!(reserved_bind_name(names.iter().copied()), Some("PATH"));
    }

    #[test]
    fn test_parse_wait_block() {
        let input = r#"# Runbook

Enter creds then wait for OTP:

```bash
./login start
```

```wait
kind: email
match:
  from: auth@example.com
  subject_contains: "verification code"
timeout: 5m
bind: otp_code
on_timeout: abort
```

```bash
echo "$otp_code" | ./login --otp
```
"#;
        let wb = parse(input);
        assert_eq!(wb.code_block_count(), 2);
        let waits: Vec<&WaitSpec> = wb
            .sections
            .iter()
            .filter_map(|s| {
                if let Section::Wait(w) = s {
                    Some(w)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(waits.len(), 1);
        let w = waits[0];
        assert_eq!(w.kind.as_deref(), Some("email"));
        assert_eq!(w.timeout.as_deref(), Some("5m"));
        assert_eq!(w.on_timeout.as_deref(), Some("abort"));
        match &w.bind {
            Some(BindSpec::Single(n)) => assert_eq!(n, "otp_code"),
            _ => panic!("expected Single bind"),
        }
    }

    #[test]
    #[test]
    fn test_parse_wait_rejects_reserved_bind() {
        let input = "```wait\nkind: email\nbind: PATH\n```\n";
        let wb = parse(input);
        let waits: Vec<&WaitSpec> = wb
            .sections
            .iter()
            .filter_map(|s| match s {
                Section::Wait(w) => Some(w),
                _ => None,
            })
            .collect();
        assert_eq!(waits.len(), 1, "wait section should still exist");
        assert!(
            waits[0].bind.is_none(),
            "reserved bind name should be cleared, not preserved"
        );
    }

    #[test]
    fn test_parse_wait_rejects_reserved_in_list() {
        let input = "```wait\nkind: email\nbind:\n  - otp\n  - WB_ARTIFACTS_DIR\n```\n";
        let wb = parse(input);
        let waits: Vec<&WaitSpec> = wb
            .sections
            .iter()
            .filter_map(|s| match s {
                Section::Wait(w) => Some(w),
                _ => None,
            })
            .collect();
        assert_eq!(waits.len(), 1);
        assert!(
            waits[0].bind.is_none(),
            "reserved name anywhere in bind list should reject the whole bind"
        );
    }

    #[test]
    fn test_parse_wait_multi_bind() {
        let input = r#"```wait
kind: manual
bind: [code, sender]
```
"#;
        let wb = parse(input);
        let w = wb
            .sections
            .iter()
            .find_map(|s| {
                if let Section::Wait(w) = s {
                    Some(w)
                } else {
                    None
                }
            })
            .unwrap();
        match &w.bind {
            Some(BindSpec::Multiple(v)) => {
                assert_eq!(v, &vec!["code".to_string(), "sender".to_string()])
            }
            _ => panic!("expected Multiple bind"),
        }
    }

    #[test]
    fn test_parse_browser_block() {
        let input = r#"# Mail check

```browser
session: ipostal1
verbs:
  - goto: https://app.ipostal1.com
  - click: "button.sign-in"
  - fill:
      selector: "input[name=email]"
      value: "{{ email }}"
  - act: "click the approve button"
```
"#;
        let wb = parse(input);
        assert_eq!(wb.code_block_count(), 1); // browser slices count as executable units
        let browsers: Vec<&BrowserSliceSpec> = wb
            .sections
            .iter()
            .filter_map(|s| {
                if let Section::Browser(b) = s {
                    Some(b)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(browsers.len(), 1);
        let b = browsers[0];
        assert_eq!(b.session.as_deref(), Some("ipostal1"));
        assert_eq!(b.verbs.len(), 4);
        assert!(b.line_number > 0);
        assert!(!b.raw.is_empty());
    }

    #[test]
    fn test_parse_browser_block_with_profile_id() {
        let input = r#"```browser
session: airbase
profile_id: 550e8400-e29b-41d4-a716-446655440000
verbs:
  - goto: https://example.com
```
"#;
        let wb = parse(input);
        let b = wb
            .sections
            .iter()
            .find_map(|s| {
                if let Section::Browser(b) = s {
                    Some(b)
                } else {
                    None
                }
            })
            .expect("browser slice");
        assert_eq!(
            b.profile.as_deref(),
            Some("550e8400-e29b-41d4-a716-446655440000")
        );
        assert_eq!(b.session.as_deref(), Some("airbase"));
    }

    #[test]
    fn test_parse_browser_block_without_profile_id() {
        let input = r#"```browser
session: airbase
verbs:
  - goto: https://example.com
```
"#;
        let wb = parse(input);
        let b = wb
            .sections
            .iter()
            .find_map(|s| {
                if let Section::Browser(b) = s {
                    Some(b)
                } else {
                    None
                }
            })
            .expect("browser slice");
        assert!(b.profile.is_none());
    }

    #[test]
    fn test_block_policy_defaults_when_unset() {
        let fm = Frontmatter::default();
        let p = fm.block_policy(1);
        assert_eq!(p.timeout_secs, None);
        assert_eq!(p.retries, 0);
        assert!(!p.continue_on_error);
    }

    #[test]
    fn test_block_policy_timeouts_retries_continue_on_error() {
        let input = r#"---
timeouts:
  1: 30s
  3: 2m
retries:
  3: 2
continue_on_error: [4]
---

```bash
echo one
```

```bash
echo two
```

```bash
echo three
```

```bash
echo four
```
"#;
        let wb = parse(input);
        // Block 1: custom timeout, no retries, not ignoring failures.
        let p1 = wb.frontmatter.block_policy(1);
        assert_eq!(p1.timeout_secs, Some(30));
        assert_eq!(p1.retries, 0);
        assert!(!p1.continue_on_error);

        // Block 2: nothing set — all defaults.
        let p2 = wb.frontmatter.block_policy(2);
        assert_eq!(p2.timeout_secs, None);
        assert_eq!(p2.retries, 0);
        assert!(!p2.continue_on_error);

        // Block 3: timeout + retries, not in continue_on_error list.
        let p3 = wb.frontmatter.block_policy(3);
        assert_eq!(p3.timeout_secs, Some(120));
        assert_eq!(p3.retries, 2);
        assert!(!p3.continue_on_error);

        // Block 4: only continue_on_error flagged.
        let p4 = wb.frontmatter.block_policy(4);
        assert_eq!(p4.timeout_secs, None);
        assert_eq!(p4.retries, 0);
        assert!(p4.continue_on_error);
    }

    #[test]
    fn test_block_policy_bad_duration_falls_back_to_default() {
        let input = r#"---
timeouts:
  1: "not-a-duration"
---

```bash
echo one
```
"#;
        let wb = parse(input);
        // Bad duration is dropped with a warning; falls through to default.
        let p = wb.frontmatter.block_policy(1);
        assert_eq!(p.timeout_secs, None);
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration_secs("30s").unwrap(), 30);
        assert_eq!(parse_duration_secs("5m").unwrap(), 300);
        assert_eq!(parse_duration_secs("2h").unwrap(), 7200);
        assert_eq!(parse_duration_secs("1d").unwrap(), 86400);
        assert_eq!(parse_duration_secs("45").unwrap(), 45);
        assert!(parse_duration_secs("x").is_err());
        assert!(parse_duration_secs("").is_err());
    }

    #[test]
    fn test_parse_requires_python() {
        let input = r#"---
title: Sandbox Test
runtime: python
requires:
  sandbox: python
  apt: [qpdf, poppler-utils]
  pip: [pikepdf, pypdf]
---

```python
print("hello")
```
"#;
        let wb = parse(input);
        let req = wb.frontmatter.requires.unwrap();
        assert_eq!(req.sandbox, "python");
        assert_eq!(req.apt, vec!["qpdf", "poppler-utils"]);
        assert_eq!(req.pip, vec!["pikepdf", "pypdf"]);
        assert!(req.node.is_empty());
        assert!(req.dockerfile.is_none());
    }

    #[test]
    fn test_parse_requires_node() {
        let input = r#"---
requires:
  sandbox: node
  apt: [chromium]
  node: ["@browserbasehq/sdk", "axios"]
---

```bash
echo "hello"
```
"#;
        let wb = parse(input);
        let req = wb.frontmatter.requires.unwrap();
        assert_eq!(req.sandbox, "node");
        assert_eq!(req.apt, vec!["chromium"]);
        assert_eq!(req.node, vec!["@browserbasehq/sdk", "axios"]);
        assert!(req.pip.is_empty());
    }

    #[test]
    fn test_parse_requires_custom() {
        let input = r#"---
requires:
  sandbox: custom
  dockerfile: ./Dockerfile.payroll
---

```bash
echo "hello"
```
"#;
        let wb = parse(input);
        let req = wb.frontmatter.requires.unwrap();
        assert_eq!(req.sandbox, "custom");
        assert_eq!(req.dockerfile.as_deref(), Some("./Dockerfile.payroll"));
    }

    #[test]
    fn test_parse_no_requires() {
        let input = r#"---
title: No Sandbox
---

```bash
echo "hello"
```
"#;
        let wb = parse(input);
        assert!(wb.frontmatter.requires.is_none());
    }

    #[test]
    fn test_non_executable_blocks_skipped() {
        let input = r#"# Config example

```yaml
key: value
```

```bash
echo "this runs"
```
"#;
        let wb = parse(input);
        assert_eq!(wb.code_block_count(), 1);
    }

    // --- info-string flag parsing ---

    #[test]
    fn test_parse_info_string_language_only() {
        let info = parse_info_string("bash");
        assert_eq!(info.language, "bash");
        assert!(!info.skip_execution);
        assert!(!info.silent);
    }

    #[test]
    fn test_parse_info_string_no_run() {
        let info = parse_info_string("bash {no-run}");
        assert_eq!(info.language, "bash");
        assert!(info.skip_execution);
        assert!(!info.silent);
    }

    #[test]
    fn test_parse_info_string_silent() {
        let info = parse_info_string("browser {silent}");
        assert_eq!(info.language, "browser");
        assert!(!info.skip_execution);
        assert!(info.silent);
    }

    #[test]
    fn test_parse_info_string_both_flags_comma_separated() {
        let info = parse_info_string("python {no-run, silent}");
        assert_eq!(info.language, "python");
        assert!(info.skip_execution);
        assert!(info.silent);
    }

    #[test]
    fn test_parse_info_string_both_flags_whitespace_separated() {
        let info = parse_info_string("python {no-run silent}");
        assert!(info.skip_execution);
        assert!(info.silent);
    }

    #[test]
    fn test_parse_info_string_unknown_flags_ignored() {
        // Forward-compatible: unknown tokens inside braces are ignored rather
        // than failing the parse, so older wb versions tolerate new flags.
        let info = parse_info_string("bash {no-run, future-flag}");
        assert_eq!(info.language, "bash");
        assert!(info.skip_execution);
        assert!(!info.silent);
    }

    #[test]
    fn test_parse_info_string_unclosed_brace_is_language() {
        // No closing `}` → whole token stays as the language (falls through
        // to is_executable_language, which rejects it). Better than silently
        // eating a malformed attribute cluster.
        let info = parse_info_string("bash {no-run");
        assert_eq!(info.language, "bash {no-run");
        assert!(!info.skip_execution);
    }

    // --- {no-run} and {silent} fence flags (stable since v0.9.8) ---

    #[test]
    fn test_no_run_excluded_from_count() {
        let input = r#"```bash {no-run}
echo "illustrative"
```

```bash
echo "runs"
```
"#;
        let wb = parse(input);
        // Only the plain bash block counts — no-run is excluded from progress.
        assert_eq!(wb.code_block_count(), 1);
        // But the no-run block is still parsed into the sections list so
        // tooling (docs renderers, wb inspect) can see it.
        let code_blocks: Vec<&CodeBlock> = wb
            .sections
            .iter()
            .filter_map(|s| {
                if let Section::Code(b) = s {
                    Some(b)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(code_blocks.len(), 2);
        assert!(code_blocks[0].skip_execution);
        assert!(!code_blocks[1].skip_execution);
    }

    #[test]
    fn test_silent_counts_toward_total() {
        let input = r#"```bash {silent}
echo "setup"
```

```bash
echo "main"
```
"#;
        let wb = parse(input);
        // Silent executes and counts — it just doesn't emit step.complete.
        assert_eq!(wb.code_block_count(), 2);
        let blocks: Vec<&CodeBlock> = wb
            .sections
            .iter()
            .filter_map(|s| {
                if let Section::Code(b) = s {
                    Some(b)
                } else {
                    None
                }
            })
            .collect();
        assert!(blocks[0].silent);
        assert!(!blocks[0].skip_execution);
    }

    #[test]
    fn test_browser_flags_parsed() {
        let input = r#"```browser {no-run}
session: airbase
verbs:
  - goto: https://example.com
```
"#;
        let wb = parse(input);
        let browsers: Vec<&BrowserSliceSpec> = wb
            .sections
            .iter()
            .filter_map(|s| {
                if let Section::Browser(b) = s {
                    Some(b)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(browsers.len(), 1);
        assert!(browsers[0].skip_execution);
        assert_eq!(wb.code_block_count(), 0);
    }

    // --- {when=…} / {skip_if=…} runtime-conditional attrs ----------------

    #[test]
    fn test_parse_info_string_when_truthy_check() {
        let info = parse_info_string("bash {when=$DEPLOY_ENV}");
        assert_eq!(info.language, "bash");
        assert_eq!(info.when.as_deref(), Some("$DEPLOY_ENV"));
        assert!(info.skip_if.is_none());
    }

    #[test]
    fn test_parse_info_string_when_equals() {
        let info = parse_info_string("bash {when=$DEPLOY_ENV=prod}");
        assert_eq!(info.when.as_deref(), Some("$DEPLOY_ENV=prod"));
    }

    #[test]
    fn test_parse_info_string_skip_if_truthy() {
        let info = parse_info_string("bash {skip_if=$DRY_RUN}");
        assert_eq!(info.skip_if.as_deref(), Some("$DRY_RUN"));
        assert!(info.when.is_none());
    }

    #[test]
    fn test_parse_info_string_conditional_combines_with_silent() {
        let info = parse_info_string("bash {when=$X, silent}");
        assert_eq!(info.when.as_deref(), Some("$X"));
        assert!(info.silent);
    }

    #[test]
    fn test_parse_info_string_negation() {
        let info = parse_info_string("bash {when=!$DRY_RUN}");
        assert_eq!(info.when.as_deref(), Some("!$DRY_RUN"));
    }

    #[test]
    fn test_parse_info_string_not_equals() {
        let info = parse_info_string("bash {skip_if=$ENV!=prod}");
        assert_eq!(info.skip_if.as_deref(), Some("$ENV!=prod"));
    }

    fn env_map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn test_evaluate_condition_truthy_missing_var() {
        assert!(!evaluate_condition("$MISSING", &env_map(&[])));
    }

    #[test]
    fn test_evaluate_condition_truthy_set_var() {
        assert!(evaluate_condition("$X", &env_map(&[("X", "1")])));
        assert!(evaluate_condition("$X", &env_map(&[("X", "anything")])));
    }

    #[test]
    fn test_evaluate_condition_falsy_values() {
        // Empty, "0", "false", "no", "off" are all falsy (case-insensitive).
        for v in ["", "0", "false", "FALSE", "No", "off"] {
            assert!(
                !evaluate_condition("$X", &env_map(&[("X", v)])),
                "expected '{}' to be falsy",
                v
            );
        }
    }

    #[test]
    fn test_evaluate_condition_equals() {
        let env = env_map(&[("DEPLOY", "prod")]);
        assert!(evaluate_condition("$DEPLOY=prod", &env));
        assert!(!evaluate_condition("$DEPLOY=staging", &env));
        // Missing var with equality: always false.
        assert!(!evaluate_condition("$MISSING=prod", &env));
    }

    #[test]
    fn test_evaluate_condition_not_equals() {
        let env = env_map(&[("DEPLOY", "prod")]);
        assert!(!evaluate_condition("$DEPLOY!=prod", &env));
        assert!(evaluate_condition("$DEPLOY!=staging", &env));
        // Missing var with !=: true (a missing var is "not equal" to anything).
        assert!(evaluate_condition("$MISSING!=prod", &env));
    }

    #[test]
    fn test_evaluate_condition_negation() {
        let env = env_map(&[("X", "1")]);
        assert!(!evaluate_condition("!$X", &env));
        assert!(evaluate_condition("!$MISSING", &env));
        // Negation composes with equality.
        assert!(!evaluate_condition("!$X=1", &env));
        assert!(evaluate_condition("!$X=2", &env));
    }

    #[test]
    fn test_evaluate_condition_malformed_is_false() {
        // Expressions not starting with `$` (or `!$`) are invalid and treated
        // as false — fail-safe: a broken conditional does not run the block.
        assert!(!evaluate_condition("DEPLOY", &env_map(&[("DEPLOY", "1")])));
        assert!(!evaluate_condition("", &env_map(&[])));
    }

    #[test]
    fn test_should_skip_block_when_only() {
        let env = env_map(&[("X", "1")]);
        assert!(should_skip_block(Some("$X"), None, &env).is_none());
        let reason = should_skip_block(Some("$MISSING"), None, &env);
        assert!(reason.is_some());
        assert!(reason.unwrap().starts_with("when="));
    }

    #[test]
    fn test_should_skip_block_skip_if_only() {
        let env = env_map(&[("DRY_RUN", "1")]);
        let reason = should_skip_block(None, Some("$DRY_RUN"), &env);
        assert!(reason.is_some());
        assert!(reason.unwrap().starts_with("skip_if="));
        assert!(should_skip_block(None, Some("$MISSING"), &env).is_none());
    }

    #[test]
    fn test_should_skip_block_when_and_skip_if_compose() {
        // Run only if X is set AND DRY_RUN is not set.
        let run = env_map(&[("X", "1")]);
        assert!(should_skip_block(Some("$X"), Some("$DRY_RUN"), &run).is_none());

        let skip_dry = env_map(&[("X", "1"), ("DRY_RUN", "1")]);
        let r = should_skip_block(Some("$X"), Some("$DRY_RUN"), &skip_dry);
        assert!(r.unwrap().starts_with("skip_if="));

        let skip_missing = env_map(&[]);
        let r = should_skip_block(Some("$X"), Some("$DRY_RUN"), &skip_missing);
        assert!(r.unwrap().starts_with("when="));
    }

    #[test]
    fn test_code_block_conditional_attrs_attached() {
        let input = r#"```bash {when=$X}
echo one
```

```bash {skip_if=$DRY_RUN}
echo two
```

```bash
echo three
```
"#;
        let wb = parse(input);
        let blocks: Vec<&CodeBlock> = wb
            .sections
            .iter()
            .filter_map(|s| if let Section::Code(b) = s { Some(b) } else { None })
            .collect();
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].when.as_deref(), Some("$X"));
        assert!(blocks[0].skip_if.is_none());
        assert!(blocks[1].when.is_none());
        assert_eq!(blocks[1].skip_if.as_deref(), Some("$DRY_RUN"));
        assert!(blocks[2].when.is_none() && blocks[2].skip_if.is_none());
        // Conditional blocks still count — can't be evaluated at parse time.
        assert_eq!(wb.code_block_count(), 3);
    }

    #[test]
    fn test_browser_slice_conditional_attrs_attached() {
        let input = r#"```browser {when=$BROWSER_ON}
session: s1
verbs: []
```
"#;
        let wb = parse(input);
        let b = wb
            .sections
            .iter()
            .find_map(|s| if let Section::Browser(b) = s { Some(b) } else { None })
            .expect("browser slice parsed");
        assert_eq!(b.when.as_deref(), Some("$BROWSER_ON"));
        assert!(b.skip_if.is_none());
    }

    // --- Include fence + resolve_includes --------------------------------

    use std::io::Write;

    /// Write `content` to a temp file rooted at `dir` and return the path.
    fn write_temp(dir: &Path, name: &str, content: &str) -> PathBuf {
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).expect("create temp");
        f.write_all(content.as_bytes()).expect("write temp");
        p
    }

    /// Fresh unique temp dir per test to avoid collisions across parallel runs.
    fn fresh_tempdir(prefix: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "wb-include-test-{}-{}-{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&base).expect("create tempdir");
        base
    }

    #[test]
    fn test_include_fence_parsed_as_section() {
        let input = r#"```include
path: ./login.md
```

```bash
echo ok
```
"#;
        let wb = parse(input);
        let has_include = wb
            .sections
            .iter()
            .any(|s| matches!(s, Section::Include(spec) if spec.path == "./login.md"));
        assert!(has_include, "include fence should parse into Section::Include");
        // Include does not contribute to code_block_count (it expands away).
        assert_eq!(wb.code_block_count(), 1);
    }

    #[test]
    fn test_resolve_includes_splices_target_sections() {
        let dir = fresh_tempdir("basic");
        write_temp(
            &dir,
            "login.md",
            r#"```bash
echo "logged in"
```

```python
print("checked session")
```
"#,
        );
        let parent_path = write_temp(
            &dir,
            "deploy.md",
            r#"```bash
echo "before login"
```

```include
path: ./login.md
```

```bash
echo "after login"
```
"#,
        );
        let wb = parse(&std::fs::read_to_string(&parent_path).unwrap());
        let resolved = resolve_includes(wb, &parent_path).expect("resolve");

        let code_langs: Vec<&str> = resolved
            .sections
            .iter()
            .filter_map(|s| match s {
                Section::Code(b) => Some(b.language.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(code_langs, vec!["bash", "bash", "python", "bash"]);
        assert!(
            resolved
                .sections
                .iter()
                .all(|s| !matches!(s, Section::Include(_))),
            "no unresolved includes should remain"
        );
        assert_eq!(resolved.code_block_count(), 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolve_includes_nested() {
        // A includes B, B includes C. All three should flatten.
        let dir = fresh_tempdir("nested");
        write_temp(&dir, "c.md", "```bash\necho C\n```\n");
        write_temp(
            &dir,
            "b.md",
            "```bash\necho B\n```\n\n```include\npath: ./c.md\n```\n",
        );
        let a = write_temp(
            &dir,
            "a.md",
            "```bash\necho A\n```\n\n```include\npath: ./b.md\n```\n",
        );
        let wb = parse(&std::fs::read_to_string(&a).unwrap());
        let resolved = resolve_includes(wb, &a).expect("nested resolve");
        assert_eq!(resolved.code_block_count(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolve_includes_detects_cycle() {
        // A → B → A: resolution must fail, not loop.
        let dir = fresh_tempdir("cycle");
        let a = write_temp(
            &dir,
            "a.md",
            "```include\npath: ./b.md\n```\n",
        );
        write_temp(
            &dir,
            "b.md",
            "```include\npath: ./a.md\n```\n",
        );
        let wb = parse(&std::fs::read_to_string(&a).unwrap());
        let err = resolve_includes(wb, &a).expect_err("should detect cycle");
        assert!(
            err.contains("circular include"),
            "expected cycle error, got: {}",
            err
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolve_includes_missing_file() {
        let dir = fresh_tempdir("missing");
        let parent = write_temp(
            &dir,
            "a.md",
            "```include\npath: ./does-not-exist.md\n```\n",
        );
        let wb = parse(&std::fs::read_to_string(&parent).unwrap());
        let err = resolve_includes(wb, &parent).expect_err("should fail on missing file");
        assert!(
            err.contains("cannot resolve path"),
            "expected resolution error, got: {}",
            err
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolve_includes_same_target_twice_is_allowed() {
        // Including the same file twice at different positions is not a cycle —
        // the ancestor set is pruned after each recursive return.
        let dir = fresh_tempdir("dup");
        write_temp(&dir, "shared.md", "```bash\necho shared\n```\n");
        let parent = write_temp(
            &dir,
            "main.md",
            r#"```include
path: ./shared.md
```

```bash
echo middle
```

```include
path: ./shared.md
```
"#,
        );
        let wb = parse(&std::fs::read_to_string(&parent).unwrap());
        let resolved = resolve_includes(wb, &parent).expect("dup include ok");
        assert_eq!(resolved.code_block_count(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolve_includes_target_path_relative_to_includer_not_cwd() {
        // An included workbook B in dir/ that itself includes ./c.md must
        // resolve c.md relative to dir/, not the parent's dir or CWD.
        let dir = fresh_tempdir("reldir");
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        write_temp(&sub, "c.md", "```bash\necho C\n```\n");
        write_temp(&sub, "b.md", "```include\npath: ./c.md\n```\n");
        let a = write_temp(&dir, "a.md", "```include\npath: ./sub/b.md\n```\n");
        let wb = parse(&std::fs::read_to_string(&a).unwrap());
        let resolved = resolve_includes(wb, &a).expect("nested-dir resolve");
        assert_eq!(resolved.code_block_count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
