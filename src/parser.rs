use crate::step_ir::FenceAttrs;
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
    /// Declarative prerequisites: paths to other workbooks that should run
    /// before this one's blocks. Sugar over a leading ` ```include``` ` fence —
    /// each entry is prepended as a synthetic `Section::Include` at position 0
    /// before include resolution. Order in the list = execution order. The
    /// included workbooks' frontmatter is ignored (parent controls
    /// runtime/secrets/env) and their `required:` lists are *not* recursively
    /// honored; treat this like a flat "needs:" list, not transitive deps.
    /// Note: distinct from `requires` (Docker sandbox config).
    pub required: Option<Vec<String>>,
    /// Optional compiled-workflow manifest. `wb` treats it as metadata: it
    /// validates declared node ids and passes compact fragments through
    /// callbacks/checkpoints, but does not interpret the workflow graph.
    pub workflow: Option<serde_json::Value>,
    /// Per-block timeout map. Numeric keys (1-based block numbers) set a
    /// per-block cap; the special `_default` key sets a runbook-wide default
    /// applied to every block that doesn't have its own override. Values are
    /// duration strings ("30s", "5m", "2h") — bare integers are seconds.
    /// When neither is set, blocks run unbounded.
    pub timeouts: Option<TimeoutsConfig>,
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
    /// Declared typed parameters. Each entry is either a full map
    /// (`type`/`default`/`required`/`one_of`/`secret`) or a scalar shorthand
    /// that becomes the default. Resolved at run start from `--param`,
    /// `--param-file`, the selected `--profile`, and declared defaults; the
    /// resolved set is injected into every cell's env and hashed into the
    /// checkpoint identity. See `crate::params`.
    pub params: Option<HashMap<String, crate::params::ParamSpec>>,
    /// Named parameter presets. `--profile <name>` selects one block of
    /// name → value pairs, applied below `--param`/`--param-file` and above
    /// declared defaults.
    pub profiles: Option<HashMap<String, HashMap<String, serde_yaml::Value>>>,
}

/// `timeouts:` frontmatter map. Mixes a runbook-wide `_default` cap with
/// per-block (1-based) overrides:
///
/// ```yaml
/// timeouts:
///   _default: 30m     # safety net for the whole workbook
///   3: 2m             # tighter cap on block 3
/// ```
///
/// Custom Deserialize because YAML maps mix integer scalar keys (block
/// numbers) with string scalar keys (`_default`), and `#[serde(flatten)]`
/// over `HashMap<u32, _>` flattens through the string-keyed `Content`
/// deserializer — which would reject `3` as a key.
#[derive(Debug, Default, Clone)]
pub struct TimeoutsConfig {
    /// Raw duration string for `_default` (e.g. "30m"). Parsed at lookup.
    pub default: Option<String>,
    /// Raw per-block duration strings, keyed by 1-based block number.
    pub blocks: HashMap<u32, String>,
}

impl<'de> Deserialize<'de> for TimeoutsConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{MapAccess, Visitor};
        use std::fmt;

        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = TimeoutsConfig;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(
                    f,
                    "a map of block id (positive int) or `_default` to duration string"
                )
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut out = TimeoutsConfig::default();
                while let Some(key) = access.next_key::<serde_yaml::Value>()? {
                    let val: String = access.next_value()?;
                    match key {
                        serde_yaml::Value::String(s) if s == "_default" => {
                            out.default = Some(val);
                        }
                        serde_yaml::Value::String(s) => {
                            if let Ok(n) = s.parse::<u32>() {
                                out.blocks.insert(n, val);
                            } else {
                                eprintln!("wb: ignoring unknown timeouts key '{}'", s);
                            }
                        }
                        serde_yaml::Value::Number(n) => match n.as_u64() {
                            Some(u) if u <= u32::MAX as u64 => {
                                out.blocks.insert(u as u32, val);
                            }
                            _ => {
                                eprintln!("wb: ignoring out-of-range timeouts key {:?}", n);
                            }
                        },
                        other => {
                            eprintln!("wb: ignoring unsupported timeouts key {:?}", other);
                        }
                    }
                }
                Ok(out)
            }
        }

        deserializer.deserialize_map(V)
    }
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
    /// `{no-cache}` info-string flag: exclude this block from `--cache` skipping
    /// (for side-effecting blocks that must run every time).
    pub no_cache: bool,
    /// Pandoc-style fence attributes: `{#id .class key=value}`. Drives stable
    /// step IDs (`#id`), classes/tags, and per-block policy (`timeout=`,
    /// `retries=`, `continue_on_error`). See `crate::step_ir`.
    pub attrs: FenceAttrs,
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
    #[serde(skip, default)]
    pub attrs: crate::step_ir::FenceAttrs,
}

/// An `expect` / `assert` fence — a non-executable section holding assertions
/// evaluated against the immediately preceding executable block's result.
/// Parsed eagerly so `wb validate` can report malformed lines (`wb-expect-001`)
/// without re-parsing. Does not consume a block index.
#[derive(Debug, Default)]
pub struct ExpectSpec {
    /// Parsed assertions paired with their source lines.
    pub assertions: Vec<(String, crate::assertion::Assertion)>,
    /// Malformed lines (each with a reason) for diagnostics.
    pub errors: Vec<String>,
    pub line_number: usize,
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
    /// Pandoc-style fence attributes — see `CodeBlock::attrs`.
    #[serde(skip, default)]
    pub attrs: FenceAttrs,
}

/// Where an `IncludeSpec` came from. `Fence` is the explicit
/// ` ```include path: X``` ` fence; `RequiredFrontmatter` is a synthesized entry
/// generated from a `required: [...]` frontmatter list. Drives error message
/// formatting so users see `required: 'login.md'` instead of
/// `include at L0: 'login.md'` for synthesized entries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum IncludeOrigin {
    #[default]
    Fence,
    RequiredFrontmatter,
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
    #[serde(skip, default)]
    pub origin: IncludeOrigin,
}

/// Identity of an included workbook, carried on `Section::IncludeEnter` /
/// `Section::IncludeExit` sentinels so the executor can emit
/// `step.started` / `step.finished` events keyed to the operator's mental
/// model of the run ("Logging in → Exporting → Syncing"), not the flattened
/// block list. `id` is the path relative to the CWD where `wb` was invoked
/// when possible, falling back to the canonical absolute path. `title` comes
/// from the included workbook's frontmatter.title (fallback: file stem).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncludeFrame {
    pub id: String,
    pub title: Option<String>,
}

#[derive(Debug)]
pub enum Section {
    Text(String),
    Code(CodeBlock),
    Wait(WaitSpec),
    Browser(BrowserSliceSpec),
    /// `expect` / `assert` fence — non-executable, evaluated against the prior
    /// block's result. Does not consume a block index.
    Expect(ExpectSpec),
    Include(IncludeSpec),
    /// Inserted by `resolve_includes` to mark the start of an included
    /// workbook's spliced sections. Non-executable — skipped by block
    /// counting, but the executor uses it to fire `step.started`.
    IncludeEnter(IncludeFrame),
    /// Inserted by `resolve_includes` to mark the end of an included
    /// workbook's spliced sections. Fires `step.finished`.
    IncludeExit(IncludeFrame),
}

#[derive(Debug)]
pub struct Workbook {
    pub frontmatter: Frontmatter,
    pub sections: Vec<Section>,
}

impl Workbook {
    /// Count of executable step slots (code blocks + browser slices).
    /// Browser slices consume a block index and show up in progress/callbacks
    /// exactly like code blocks do. Blocks flagged `{no-run}` now count as
    /// skipped step slots so callback progress can mark them terminal.
    pub fn code_block_count(&self) -> usize {
        self.sections
            .iter()
            .filter(|s| matches!(s, Section::Code(_) | Section::Browser(_)))
            .count()
    }

    /// Materialize the executable section list as a `Vec<Step>` with stable
    /// ids. `{no-run}` blocks are included as skipped step slots so they can
    /// emit `step.skipped`. The resulting slice is index-aligned with the
    /// run-loop's iteration over `Section::Code | Browser` (filtered on
    /// every code/browser section), which means `steps[block_idx]` matches
    /// the legacy `(block_idx + 1)` block number used by the frontmatter
    /// policy maps.
    ///
    /// Step ids are deterministic — the same workbook produces the same ids
    /// on every parse. See `crate::step_ir` for the hashing rules.
    pub fn build_steps(&self) -> Vec<crate::step_ir::Step> {
        use crate::step_ir::{IncludeFrame as StepFrame, Source, Span, Step};
        let mut steps = Vec::new();
        let mut chain: Vec<StepFrame> = Vec::new();
        // Position counter per scope. `scope_positions[0]` is the root file,
        // each pushed include frame appends its own counter.
        let mut scope_positions: Vec<u32> = vec![0];
        for section in &self.sections {
            match section {
                Section::IncludeEnter(frame) => {
                    let call_site = *scope_positions.last().unwrap_or(&0);
                    chain.push(StepFrame {
                        id: frame.id.clone(),
                        title: frame.title.clone(),
                        call_site,
                    });
                    scope_positions.push(0);
                }
                Section::IncludeExit(_) => {
                    chain.pop();
                    scope_positions.pop();
                }
                Section::Code(b) => {
                    let position = *scope_positions.last().unwrap_or(&0);
                    let id = Step::compute_id(
                        &chain,
                        position,
                        &b.language,
                        &b.code,
                        b.attrs.explicit_id.as_deref(),
                    );
                    steps.push(Step {
                        id,
                        attrs: b.attrs.clone(),
                        span: Span::point(b.line_number as u32),
                        source: Source {
                            file: PathBuf::new(),
                            position,
                        },
                        language: b.language.clone(),
                        body: b.code.clone(),
                        include_chain: chain.clone(),
                    });
                    if let Some(p) = scope_positions.last_mut() {
                        *p += 1;
                    }
                }
                Section::Browser(spec) => {
                    let position = *scope_positions.last().unwrap_or(&0);
                    let id = Step::compute_id(
                        &chain,
                        position,
                        "browser",
                        &spec.raw,
                        spec.attrs.explicit_id.as_deref(),
                    );
                    steps.push(Step {
                        id,
                        attrs: spec.attrs.clone(),
                        span: Span::point(spec.line_number as u32),
                        source: Source {
                            file: PathBuf::new(),
                            position,
                        },
                        language: "browser".to_string(),
                        body: spec.raw.clone(),
                        include_chain: chain.clone(),
                    });
                    if let Some(p) = scope_positions.last_mut() {
                        *p += 1;
                    }
                }
                _ => {}
            }
        }
        steps
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
pub fn resolve_includes(wb: Workbook, parent_path: &Path) -> crate::error::WbResult<Workbook> {
    let parent_canonical = parent_path.canonicalize().map_err(|e| {
        crate::error::WbError::Workbook(format!(
            "cannot resolve workbook path {}: {}",
            parent_path.display(),
            e
        ))
    })?;
    let mut visiting = HashSet::new();
    visiting.insert(parent_canonical.clone());
    let base_dir = parent_canonical
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    // Root for step_id computation: CWD where `wb` was invoked falls back
    // to the top-level workbook's directory if CWD isn't an ancestor. This
    // keeps ids readable (`services/airbase/login.md`) rather than canonical
    // absolute paths.
    let id_root = std::env::current_dir().unwrap_or_else(|_| base_dir.clone());

    // `required:` sugar — prepend each entry as a synthetic include at
    // position 0 so prerequisites run before the parent's first block. Reuses
    // the include pipeline for cycle detection, path resolution, and
    // IncludeEnter/Exit sentinels. Inner workbooks' `required:` is ignored
    // (their frontmatter is ignored entirely, matching the include-fence
    // contract).
    let sections = match &wb.frontmatter.required {
        Some(reqs) if !reqs.is_empty() => {
            let mut prefix: Vec<Section> = reqs
                .iter()
                .map(|p| {
                    Section::Include(IncludeSpec {
                        path: p.clone(),
                        line_number: 0,
                        section_index: 0,
                        origin: IncludeOrigin::RequiredFrontmatter,
                    })
                })
                .collect();
            prefix.extend(wb.sections);
            prefix
        }
        _ => wb.sections,
    };

    let resolved = resolve_sections(sections, &base_dir, &id_root, &mut visiting)?;
    Ok(Workbook {
        frontmatter: wb.frontmatter,
        sections: resolved,
    })
}

/// Human-readable error prefix for an `IncludeSpec` — distinguishes
/// `required:` frontmatter entries from explicit include fences so missing-file
/// / cycle / read errors point at the user's actual source of the include.
fn include_origin_label(spec: &IncludeSpec) -> String {
    match spec.origin {
        IncludeOrigin::Fence => format!("include at L{}", spec.line_number),
        IncludeOrigin::RequiredFrontmatter => format!("required '{}'", spec.path),
    }
}

/// Compute a readable step_id for an included workbook: path relative to
/// `id_root` if possible, otherwise the full canonical path. Used as the
/// stable identifier in `step.started` / `step.finished` events.
fn compute_step_id(canonical: &Path, id_root: &Path) -> String {
    canonical
        .strip_prefix(id_root)
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| canonical.to_string_lossy().into_owned())
}

fn resolve_sections(
    sections: Vec<Section>,
    base_dir: &Path,
    id_root: &Path,
    visiting: &mut HashSet<PathBuf>,
) -> crate::error::WbResult<Vec<Section>> {
    use crate::error::WbError;
    let mut out = Vec::new();
    for section in sections {
        match section {
            Section::Include(spec) => {
                let where_ = include_origin_label(&spec);
                let target = base_dir.join(&spec.path);
                let target_canonical = target.canonicalize().map_err(|e| {
                    WbError::Workbook(format!(
                        "{}: cannot resolve path '{}' (relative to {}): {}",
                        where_,
                        spec.path,
                        base_dir.display(),
                        e
                    ))
                })?;
                if visiting.contains(&target_canonical) {
                    return Err(WbError::Workbook(format!(
                        "{}: circular include of '{}' (already being resolved)",
                        where_,
                        target_canonical.display()
                    )));
                }
                let content = fs::read_to_string(&target_canonical).map_err(|e| {
                    WbError::Workbook(format!(
                        "{}: cannot read '{}': {}",
                        where_,
                        target_canonical.display(),
                        e
                    ))
                })?;
                let inner = parse(&content);
                let inner_base = target_canonical
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));
                let frame = IncludeFrame {
                    id: compute_step_id(&target_canonical, id_root),
                    title: inner.frontmatter.title.clone().or_else(|| {
                        target_canonical
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                    }),
                };
                visiting.insert(target_canonical.clone());
                let inner_resolved =
                    resolve_sections(inner.sections, &inner_base, id_root, visiting)?;
                visiting.remove(&target_canonical);
                out.push(Section::IncludeEnter(frame.clone()));
                out.extend(inner_resolved);
                out.push(Section::IncludeExit(frame));
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
                    crate::log_warn!("wb: frontmatter parse warning: {}", e);
                    Frontmatter::default()
                }
            };
            (frontmatter, rest.to_string())
        }
        None => (Frontmatter::default(), input.to_string()),
    }
}

/// Parsed info string: language token + Pandoc-style fence attrs.
///
/// Recognized attrs:
///   - bare flags: `no-run`, `silent`, `continue_on_error`
///   - explicit step id: `#id` (Pandoc-style)
///   - class/tag: `.class`
///   - runtime conditionals: `when=EXPR`, `skip_if=EXPR`
///   - kv attrs: any other `key=value` (e.g. `timeout=30s`, `retries=2`)
#[derive(Debug, Default, PartialEq, Eq)]
struct InfoString {
    language: String,
    skip_execution: bool,
    silent: bool,
    /// `when=EXPR` — run only if EXPR is truthy at runtime.
    when: Option<String>,
    /// `skip_if=EXPR` — skip if EXPR is truthy at runtime.
    skip_if: Option<String>,
    /// `no-cache` — exclude this block from `--cache` skipping.
    no_cache: bool,
    /// `#id`, `.class`, and `key=value` attrs.
    attrs: FenceAttrs,
}

/// Split a fence info string like `bash {#login .critical timeout=30s}` into
/// language + attrs. Brace cluster is optional and can appear anywhere after
/// the language token. Attrs inside braces are comma or whitespace separated;
/// unknown bare attrs are ignored so the parser stays forward-compatible.
/// Key/value attrs must have no whitespace in the value — the split would
/// fracture the expression.
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
            if let Some(id) = flag.strip_prefix('#') {
                if !id.is_empty() {
                    out.attrs.explicit_id = Some(id.to_string());
                }
            } else if let Some(class) = flag.strip_prefix('.') {
                if !class.is_empty() {
                    out.attrs.classes.push(class.to_string());
                }
            } else if let Some(expr) = flag.strip_prefix("when=") {
                out.when = Some(expr.to_string());
            } else if let Some(expr) = flag.strip_prefix("skip_if=") {
                out.skip_if = Some(expr.to_string());
            } else if let Some((key, value)) = flag.split_once('=') {
                if !key.is_empty() {
                    out.attrs.kv.insert(key.to_string(), value.to_string());
                }
            } else {
                match flag {
                    "no-run" => out.skip_execution = true,
                    "silent" => out.silent = true,
                    "no-cache" => out.no_cache = true,
                    "continue_on_error" => {
                        out.attrs
                            .kv
                            .insert("continue_on_error".into(), "true".into());
                    }
                    // Retain unrecognized bare flags so `wb validate` can flag
                    // them (`wb-attr-001`); the runtime still ignores them.
                    other => out.attrs.unknown.push(other.to_string()),
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
    if negate {
        !result
    } else {
        result
    }
}

fn is_truthy(v: Option<&String>) -> bool {
    match v {
        None => false,
        Some(s) => {
            let t = s.trim();
            if t.is_empty() {
                return false;
            }
            !matches!(
                t.to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
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
                spec.attrs = info.attrs.clone();
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
                        eprintln!("wb: include block parse error at L{}: {}", line_num + 1, e);
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
                spec.attrs = info.attrs.clone();
                sections.push(Section::Browser(spec));
                continue;
            }

            // Expect/assert fence: a non-executable assertion block evaluated
            // against the previous executable block. Parsed eagerly into
            // assertions + errors.
            if language.eq_ignore_ascii_case("expect") || language.eq_ignore_ascii_case("assert") {
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
                let parsed = crate::assertion::parse(&body_lines.join("\n"));
                sections.push(Section::Expect(ExpectSpec {
                    assertions: parsed.assertions,
                    errors: parsed.errors,
                    line_number: line_num + 1,
                }));
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
                no_cache: info.no_cache,
                attrs: info.attrs,
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
    /// Resolve the runbook-wide default timeout from `timeouts._default`.
    /// Returns `None` if unset or the value fails to parse (parse failures
    /// log a warning and fall through to "no default").
    pub fn default_block_timeout_secs(&self) -> Option<u64> {
        self.timeouts
            .as_ref()
            .and_then(|t| t.default.as_deref())
            .and_then(|s| match parse_duration_secs(s) {
                Ok(n) => Some(n),
                Err(e) => {
                    eprintln!("wb: ignoring invalid timeouts._default='{}': {}", s, e);
                    None
                }
            })
    }

    /// Resolve the per-block policy for a given 1-based block number.
    /// Unknown numbers return the all-default policy.
    ///
    /// Superseded by `crate::step_ir::resolve_step_policies` for the run path,
    /// which folds in fence-attr overrides (`{timeout=30s}` etc). Kept as a
    /// thin lookup for tests and any tooling that still indexes by block
    /// number.
    #[allow(dead_code)]
    pub fn block_policy(&self, block_number: u32) -> BlockPolicy {
        let timeout_secs = self
            .timeouts
            .as_ref()
            .and_then(|m| m.blocks.get(&block_number))
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
pub fn parse_duration_secs(s: &str) -> crate::error::WbResult<u64> {
    use crate::error::WbError;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err(WbError::Parse("empty duration".to_string()));
    }
    let (num_part, unit) = match trimmed.chars().last() {
        Some(c) if c.is_ascii_alphabetic() => (&trimmed[..trimmed.len() - 1], c),
        _ => (trimmed, 's'),
    };
    let n: u64 = num_part
        .trim()
        .parse()
        .map_err(|_| WbError::Parse(format!("invalid duration '{}'", s)))?;
    let mult = match unit.to_ascii_lowercase() {
        's' => 1,
        'm' => 60,
        'h' => 60 * 60,
        'd' => 60 * 60 * 24,
        _ => return Err(WbError::Parse(format!("unknown duration unit '{}'", unit))),
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
            // `http` is a native runtime (REST calls via curl); executed by
            // the executor's `execute_http` path, not a language subprocess.
            | "http"
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
        let names = ["otp_code", "PATH", "sender"];
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
    fn test_parse_wait_preserves_explicit_id() {
        let input = "```wait {#approval}\nkind: manual\nbind: approved\n```\n";
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
        assert_eq!(waits[0].attrs.explicit_id.as_deref(), Some("approval"));
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
    fn test_timeouts_default_key_parses_alongside_block_keys() {
        // The `_default` key sets a runbook-wide cap; numeric keys still
        // attach to specific blocks. Verify both land in the right slots.
        let input = r#"---
timeouts:
  _default: 10m
  3: 2m
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
"#;
        let wb = parse(input);
        assert_eq!(wb.frontmatter.default_block_timeout_secs(), Some(600));
        // Block 1, 2: no per-block override.
        assert_eq!(wb.frontmatter.block_policy(1).timeout_secs, None);
        assert_eq!(wb.frontmatter.block_policy(2).timeout_secs, None);
        // Block 3: per-block override beats `_default`.
        assert_eq!(wb.frontmatter.block_policy(3).timeout_secs, Some(120));
    }

    #[test]
    fn test_timeouts_default_only() {
        // No per-block entries — just a runbook-wide cap.
        let input = r#"---
timeouts:
  _default: 45s
---

```bash
echo hi
```
"#;
        let wb = parse(input);
        assert_eq!(wb.frontmatter.default_block_timeout_secs(), Some(45));
        assert_eq!(wb.frontmatter.block_policy(1).timeout_secs, None);
    }

    #[test]
    fn test_timeouts_default_bad_duration_falls_through() {
        let input = r#"---
timeouts:
  _default: "not-a-duration"
---

```bash
echo hi
```
"#;
        let wb = parse(input);
        // Bad duration is dropped with a warning; falls through to None.
        assert_eq!(wb.frontmatter.default_block_timeout_secs(), None);
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

    // --- Pandoc-style fence attrs: {#id .class key=value} ---

    #[test]
    fn test_parse_info_string_explicit_id() {
        let info = parse_info_string("bash {#login}");
        assert_eq!(info.language, "bash");
        assert_eq!(info.attrs.explicit_id.as_deref(), Some("login"));
    }

    #[test]
    fn test_parse_info_string_class() {
        let info = parse_info_string("bash {.critical}");
        assert_eq!(info.attrs.classes, vec!["critical".to_string()]);
    }

    #[test]
    fn test_parse_info_string_kv_attrs() {
        let info = parse_info_string("bash {timeout=30s retries=2}");
        assert_eq!(info.attrs.kv.get("timeout"), Some(&"30s".to_string()));
        assert_eq!(info.attrs.kv.get("retries"), Some(&"2".to_string()));
    }

    #[test]
    fn test_parse_info_string_continue_on_error_bare_flag() {
        let info = parse_info_string("bash {continue_on_error}");
        // Bare `continue_on_error` normalizes to kv=true so policy resolver
        // can treat it the same as the explicit form.
        assert_eq!(
            info.attrs.kv.get("continue_on_error"),
            Some(&"true".to_string())
        );
    }

    #[test]
    fn test_parse_info_string_full_pandoc_cluster() {
        let info = parse_info_string("python {#deploy .critical timeout=2m retries=1}");
        assert_eq!(info.language, "python");
        assert_eq!(info.attrs.explicit_id.as_deref(), Some("deploy"));
        assert_eq!(info.attrs.classes, vec!["critical".to_string()]);
        assert_eq!(info.attrs.kv.get("timeout"), Some(&"2m".to_string()));
        assert_eq!(info.attrs.kv.get("retries"), Some(&"1".to_string()));
    }

    #[test]
    fn test_parse_info_string_preserves_existing_when_skip_if() {
        // Pandoc attrs coexist with the legacy `when=`/`skip_if=` keys.
        let info = parse_info_string("bash {#health when=$DEPLOY=prod}");
        assert_eq!(info.attrs.explicit_id.as_deref(), Some("health"));
        assert_eq!(info.when.as_deref(), Some("$DEPLOY=prod"));
    }

    #[test]
    fn test_build_steps_assigns_explicit_ids() {
        let input = r#"```bash {#login}
echo first
```

```bash
echo second
```
"#;
        let wb = parse(input);
        let steps = wb.build_steps();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].id.as_str(), "login");
        assert!(steps[1].id.as_str().starts_with("auto-"));
    }

    #[test]
    fn test_build_steps_includes_no_run_blocks_for_skip_events() {
        let input = r#"```bash {no-run}
echo skipped
```

```bash
echo runs
```
"#;
        let wb = parse(input);
        let steps = wb.build_steps();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].body, "echo skipped");
        assert_eq!(steps[1].body, "echo runs");
    }

    #[test]
    fn test_build_steps_deterministic_across_parses() {
        let input = r#"```bash
echo one
```

```python
print("two")
```
"#;
        let a = parse(input).build_steps();
        let b = parse(input).build_steps();
        assert_eq!(a.len(), b.len());
        assert_eq!(a[0].id, b[0].id);
        assert_eq!(a[1].id, b[1].id);
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
    fn test_no_run_counts_as_skipped_step_slot() {
        let input = r#"```bash {no-run}
echo "illustrative"
```

```bash
echo "runs"
```
"#;
        let wb = parse(input);
        // `{no-run}` now counts as a terminal skipped step for callback
        // progress; it is parsed but not executed.
        assert_eq!(wb.code_block_count(), 2);
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
        assert_eq!(wb.code_block_count(), 1);
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
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
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
            .filter_map(|s| {
                if let Section::Code(b) = s {
                    Some(b)
                } else {
                    None
                }
            })
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
            .find_map(|s| {
                if let Section::Browser(b) = s {
                    Some(b)
                } else {
                    None
                }
            })
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
        assert!(
            has_include,
            "include fence should parse into Section::Include"
        );
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
        let a = write_temp(&dir, "a.md", "```include\npath: ./b.md\n```\n");
        write_temp(&dir, "b.md", "```include\npath: ./a.md\n```\n");
        let wb = parse(&std::fs::read_to_string(&a).unwrap());
        let err = resolve_includes(wb, &a)
            .expect_err("should detect cycle")
            .to_string();
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
        let parent = write_temp(&dir, "a.md", "```include\npath: ./does-not-exist.md\n```\n");
        let wb = parse(&std::fs::read_to_string(&parent).unwrap());
        let err = resolve_includes(wb, &parent)
            .expect_err("should fail on missing file")
            .to_string();
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

    #[test]
    fn test_resolve_includes_emits_enter_exit_sentinels() {
        let dir = fresh_tempdir("sentinels");
        write_temp(
            &dir,
            "login.md",
            "---\ntitle: Airbase login\n---\n\n```bash\necho logged-in\n```\n",
        );
        let parent = write_temp(
            &dir,
            "task.md",
            "```bash\necho before\n```\n\n```include\npath: ./login.md\n```\n\n```bash\necho after\n```\n",
        );
        let wb = parse(&std::fs::read_to_string(&parent).unwrap());
        let resolved = resolve_includes(wb, &parent).expect("resolve");

        // Pull out the executable + sentinel stream (drop Text sections).
        let kinds: Vec<&'static str> = resolved
            .sections
            .iter()
            .filter_map(|s| match s {
                Section::Code(_) => Some("code"),
                Section::IncludeEnter(_) => Some("enter"),
                Section::IncludeExit(_) => Some("exit"),
                _ => None,
            })
            .collect();
        assert_eq!(kinds, vec!["code", "enter", "code", "exit", "code"]);

        // Exit frame must match Enter frame (same id + title).
        let (enter_id, enter_title) = resolved
            .sections
            .iter()
            .find_map(|s| match s {
                Section::IncludeEnter(f) => Some((f.id.clone(), f.title.clone())),
                _ => None,
            })
            .unwrap();
        assert!(enter_id.ends_with("login.md"), "id: {}", enter_id);
        assert_eq!(enter_title.as_deref(), Some("Airbase login"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolve_includes_nested_sentinel_nesting() {
        // A includes B includes C → sentinels must nest: enterB, enterC, code, exitC, exitB
        let dir = fresh_tempdir("nested_sent");
        write_temp(&dir, "c.md", "---\ntitle: C\n---\n\n```bash\necho C\n```\n");
        write_temp(
            &dir,
            "b.md",
            "---\ntitle: B\n---\n\n```include\npath: ./c.md\n```\n",
        );
        let a = write_temp(
            &dir,
            "a.md",
            "---\ntitle: A\n---\n\n```include\npath: ./b.md\n```\n",
        );
        let wb = parse(&std::fs::read_to_string(&a).unwrap());
        let resolved = resolve_includes(wb, &a).expect("resolve");

        let trace: Vec<String> = resolved
            .sections
            .iter()
            .filter_map(|s| match s {
                Section::IncludeEnter(f) => {
                    Some(format!("enter:{}", f.title.as_deref().unwrap_or("?")))
                }
                Section::IncludeExit(f) => {
                    Some(format!("exit:{}", f.title.as_deref().unwrap_or("?")))
                }
                Section::Code(b) => Some(format!("code:{}", b.language)),
                _ => None,
            })
            .collect();
        assert_eq!(
            trace,
            vec!["enter:B", "enter:C", "code:bash", "exit:C", "exit:B"]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolve_includes_title_fallback_to_stem() {
        let dir = fresh_tempdir("title_fallback");
        write_temp(&dir, "login.md", "```bash\necho hi\n```\n");
        let parent = write_temp(&dir, "t.md", "```include\npath: ./login.md\n```\n");
        let wb = parse(&std::fs::read_to_string(&parent).unwrap());
        let resolved = resolve_includes(wb, &parent).expect("resolve");
        let title = resolved
            .sections
            .iter()
            .find_map(|s| match s {
                Section::IncludeEnter(f) => Some(f.title.clone()),
                _ => None,
            })
            .flatten();
        assert_eq!(title.as_deref(), Some("login"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolve_includes_same_target_twice_has_matched_sentinels() {
        let dir = fresh_tempdir("dup_sent");
        write_temp(&dir, "s.md", "```bash\necho s\n```\n");
        let parent = write_temp(
            &dir,
            "main.md",
            "```include\npath: ./s.md\n```\n\n```include\npath: ./s.md\n```\n",
        );
        let wb = parse(&std::fs::read_to_string(&parent).unwrap());
        let resolved = resolve_includes(wb, &parent).expect("resolve");
        let enters = resolved
            .sections
            .iter()
            .filter(|s| matches!(s, Section::IncludeEnter(_)))
            .count();
        let exits = resolved
            .sections
            .iter()
            .filter(|s| matches!(s, Section::IncludeExit(_)))
            .count();
        assert_eq!(enters, 2);
        assert_eq!(exits, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- `required:` frontmatter --------------------------------------------

    #[test]
    fn test_required_frontmatter_prepends_includes_in_order() {
        let dir = fresh_tempdir("required_basic");
        write_temp(&dir, "a.md", "```bash\necho A\n```\n");
        write_temp(&dir, "b.md", "```bash\necho B\n```\n");
        let parent = write_temp(
            &dir,
            "main.md",
            r#"---
required:
  - ./a.md
  - ./b.md
---

```bash
echo MAIN
```
"#,
        );
        let wb = parse(&std::fs::read_to_string(&parent).unwrap());
        let resolved = resolve_includes(wb, &parent).expect("resolve");
        // Trace shows execution order: a first, then b, then main.
        let trace: Vec<String> = resolved
            .sections
            .iter()
            .filter_map(|s| match s {
                Section::Code(b) => Some(format!("code:{}", b.code.trim())),
                Section::IncludeEnter(f) => Some(format!("enter:{}", f.id)),
                Section::IncludeExit(f) => Some(format!("exit:{}", f.id)),
                _ => None,
            })
            .collect();
        // Expect: enter a, code A, exit a, enter b, code B, exit b, code MAIN
        assert!(
            trace[0].starts_with("enter:") && trace[0].ends_with("a.md"),
            "{trace:?}"
        );
        assert_eq!(trace[1], "code:echo A");
        assert!(
            trace[2].starts_with("exit:") && trace[2].ends_with("a.md"),
            "{trace:?}"
        );
        assert!(
            trace[3].starts_with("enter:") && trace[3].ends_with("b.md"),
            "{trace:?}"
        );
        assert_eq!(trace[4], "code:echo B");
        assert!(
            trace[5].starts_with("exit:") && trace[5].ends_with("b.md"),
            "{trace:?}"
        );
        assert_eq!(trace[6], "code:echo MAIN");
        assert_eq!(resolved.code_block_count(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_required_composes_with_explicit_include_fence() {
        // `required:` runs first, then any explicit `include` fences in body order.
        let dir = fresh_tempdir("required_with_fence");
        write_temp(&dir, "pre.md", "```bash\necho PRE\n```\n");
        write_temp(&dir, "mid.md", "```bash\necho MID\n```\n");
        let parent = write_temp(
            &dir,
            "main.md",
            r#"---
required: [./pre.md]
---

```bash
echo before-include
```

```include
path: ./mid.md
```

```bash
echo after-include
```
"#,
        );
        let wb = parse(&std::fs::read_to_string(&parent).unwrap());
        let resolved = resolve_includes(wb, &parent).expect("resolve");
        let codes: Vec<&str> = resolved
            .sections
            .iter()
            .filter_map(|s| match s {
                Section::Code(b) => Some(b.code.trim()),
                _ => None,
            })
            .collect();
        assert_eq!(
            codes,
            vec![
                "echo PRE",
                "echo before-include",
                "echo MID",
                "echo after-include"
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_required_missing_target_errors() {
        let dir = fresh_tempdir("required_missing");
        let parent = write_temp(
            &dir,
            "main.md",
            "---\nrequired: [./nope.md]\n---\n\n```bash\necho hi\n```\n",
        );
        let wb = parse(&std::fs::read_to_string(&parent).unwrap());
        let err = resolve_includes(wb, &parent)
            .expect_err("missing required")
            .to_string();
        // Error mentions "required" not "include at L0".
        assert!(err.contains("required"), "error: {err}");
        assert!(err.contains("nope.md"), "error: {err}");
        assert!(!err.contains("L0"), "L0 leaked into error: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_required_circular_detected() {
        // main.md required=[a.md], a.md required=[main.md] — wait, actually
        // inner workbooks' `required:` is intentionally NOT honored. So the
        // cycle here has to be via include fence inside the prerequisite.
        let dir = fresh_tempdir("required_cycle");
        write_temp(&dir, "a.md", "```include\npath: ./main.md\n```\n");
        let parent = write_temp(
            &dir,
            "main.md",
            "---\nrequired: [./a.md]\n---\n\n```bash\necho hi\n```\n",
        );
        let wb = parse(&std::fs::read_to_string(&parent).unwrap());
        let err = resolve_includes(wb, &parent)
            .expect_err("should detect cycle")
            .to_string();
        assert!(err.contains("circular"), "error: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_required_in_included_workbook_is_ignored() {
        // Parent includes B. B has its own `required: [c.md]`. That `required`
        // is in B's frontmatter, which is intentionally ignored when B is
        // included from a parent — same contract as runtime/secrets/env.
        // c.md should NOT run when we resolve via the parent.
        let dir = fresh_tempdir("required_inner_ignored");
        write_temp(&dir, "c.md", "```bash\necho C\n```\n");
        write_temp(
            &dir,
            "b.md",
            "---\nrequired: [./c.md]\n---\n\n```bash\necho B\n```\n",
        );
        let parent = write_temp(&dir, "main.md", "```include\npath: ./b.md\n```\n");
        let wb = parse(&std::fs::read_to_string(&parent).unwrap());
        let resolved = resolve_includes(wb, &parent).expect("resolve");
        let codes: Vec<&str> = resolved
            .sections
            .iter()
            .filter_map(|s| match s {
                Section::Code(b) => Some(b.code.trim()),
                _ => None,
            })
            .collect();
        // Only B ran; C was skipped because B's required: was ignored.
        assert_eq!(codes, vec!["echo B"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_required_empty_list_is_noop() {
        let dir = fresh_tempdir("required_empty");
        let parent = write_temp(
            &dir,
            "main.md",
            "---\nrequired: []\n---\n\n```bash\necho hi\n```\n",
        );
        let wb = parse(&std::fs::read_to_string(&parent).unwrap());
        let resolved = resolve_includes(wb, &parent).expect("resolve");
        assert_eq!(resolved.code_block_count(), 1);
        // No IncludeEnter sentinels.
        assert!(!resolved
            .sections
            .iter()
            .any(|s| matches!(s, Section::IncludeEnter(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
