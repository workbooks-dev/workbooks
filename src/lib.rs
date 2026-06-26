mod artifacts;
pub mod assertion;
mod atomic_io;
mod cache;
mod callback;
mod checkpoint;
mod config;
pub mod diagnostic;
mod doctor;
pub mod error;
mod executor;
mod exit;
mod exit_codes;
mod lockfile;
mod logging;
mod mcp;
mod output;
pub mod params;
pub mod parser;
mod pending;
mod sandbox;
mod secrets;
mod sidecar;
mod signal;
mod signing;
pub mod step_ir;
mod step_outputs;
mod trust;
mod update;
mod validate;
mod workflow;

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

/// Delay between retry attempts for per-block `retries:`. Short enough to
/// not feel sluggish, long enough to let a transient HTTP/API blip clear.
const RETRY_DELAY: Duration = Duration::from_millis(500);

/// Parse workbook content and expand any ```include``` fences. Exits with
/// `EXIT_WORKBOOK_INVALID` on any include resolution failure (missing target,
/// cycle, unreadable file). Every codepath that consumes a workbook for
/// execution, inspection, or transformation must go through this — downstream
/// dispatch panics on `Section::Include`.
fn parse_and_resolve(content: &str, file: &str) -> parser::Workbook {
    let wb = parser::parse(content);
    match parser::resolve_includes(wb, Path::new(file)) {
        Ok(resolved) => resolved,
        Err(e) => {
            // The exit code rides on the error category (`WbError::Workbook`
            // → `EXIT_WORKBOOK_INVALID`) rather than being hardcoded here.
            let exit = WbExit::from(e);
            if let Some(msg) = exit.message() {
                eprintln!("wb: {}", msg);
            }
            std::process::exit(exit.code());
        }
    }
}

/// Execute a code block once, applying the per-block timeout override if
/// set, and retrying on failure up to `policy.retries` times with a small
/// delay between attempts. Returns the result of the final attempt.
///
/// The session's block_timeout is always set before each attempt (to either
/// the per-block override or the run-wide default) so state can't leak
/// across blocks with different timeouts.
/// Look up the resolved per-step policy for `block_idx` and translate it to
/// the `parser::BlockPolicy` shape used by `execute_block_with_policy`. Falls
/// back to defaults when the index is out of bounds — that only happens if
/// the workbook somehow advanced past `steps.len()`, which would already be
/// a serious bug.
fn step_policy_for(
    resolved: &[step_ir::ResolvedStepPolicy],
    block_idx: usize,
) -> parser::BlockPolicy {
    let p = resolved
        .get(block_idx)
        .map(|r| r.policy)
        .unwrap_or_default();
    parser::BlockPolicy {
        timeout_secs: p.timeout_secs,
        retries: p.retries,
        continue_on_error: p.continue_on_error,
    }
}

/// Why this run picked the default block timeout it did. Threaded through
/// to `execute_block_with_policy` so the timeout error message can point
/// the user at the right knob to tune.
#[derive(Debug, Clone, Copy)]
enum DefaultTimeoutSource {
    /// `timeouts._default` in the workbook frontmatter.
    FrontmatterDefault,
    /// `--default-block-timeout` on the CLI.
    Cli,
}

impl DefaultTimeoutSource {
    fn describe(self) -> &'static str {
        match self {
            DefaultTimeoutSource::FrontmatterDefault => "frontmatter `timeouts._default`",
            DefaultTimeoutSource::Cli => "--default-block-timeout",
        }
    }
}

fn print_timeout_diagnostic(
    block: &parser::CodeBlock,
    block_idx: usize,
    used_timeout: Duration,
    per_block_source: Option<&'static str>,
    default_source: Option<DefaultTimeoutSource>,
) {
    let dur_str = format_duration(used_timeout);
    let one_based = block_idx + 1;
    let source_str = match per_block_source {
        Some(src) => src.to_string(),
        None => default_source
            .map(|s| s.describe().to_string())
            .unwrap_or_else(|| "(unknown)".to_string()),
    };
    eprintln!(
        "wb: block {} ({}) at line {} timed out after {} — limit set by {}",
        one_based, block.language, block.line_number, dur_str, source_str
    );
    eprintln!(
        "    to extend this block specifically, add to frontmatter:\n      timeouts:\n        {}: {}",
        one_based,
        format_suggested_extension(used_timeout)
    );
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= 3600 && secs.is_multiple_of(3600) {
        format!("{}h", secs / 3600)
    } else if secs >= 60 && secs.is_multiple_of(60) {
        format!("{}m", secs / 60)
    } else {
        format!("{}s", secs)
    }
}

fn format_suggested_extension(d: Duration) -> String {
    // Suggest something noticeably longer than the cap that fired, so a
    // copy-pasted entry has a chance of clearing the block in a re-run.
    let secs = d.as_secs().max(1);
    let suggested = secs.saturating_mul(3);
    format_duration(Duration::from_secs(suggested))
}

#[allow(clippy::too_many_arguments)]
fn execute_block_with_policy(
    session: &mut executor::Session,
    block: &parser::CodeBlock,
    block_idx: usize,
    policy: parser::BlockPolicy,
    default_timeout: Option<Duration>,
    default_source: Option<DefaultTimeoutSource>,
    quiet: bool,
) -> executor::BlockResult {
    // Precedence: per-block (fence-attr or frontmatter[N]) > resolved
    // run-wide default (frontmatter._default or --default-block-timeout) >
    // None (unbounded).
    let timeout = policy
        .timeout_secs
        .map(Duration::from_secs)
        .or(default_timeout);
    session.set_block_timeout(timeout);
    // For the diagnostic: if the fence attr carried `{timeout=...}` we
    // know the per-block source; otherwise the frontmatter map set it.
    // The `_default` and CLI cases are captured by `default_source`.
    let per_block_source: Option<&'static str> = if policy.timeout_secs.is_some() {
        if block.attrs.kv.contains_key("timeout") {
            Some("fence attr `{timeout=…}`")
        } else {
            Some("frontmatter `timeouts.<N>`")
        }
    } else {
        None
    };

    let mut attempt: u32 = 0;
    let total_attempts = 1 + policy.retries;
    loop {
        let result = session.execute_block(block, block_idx);
        attempt += 1;
        if result.error_type.as_deref() == Some("timeout") {
            if let Some(t) = timeout {
                print_timeout_diagnostic(block, block_idx, t, per_block_source, default_source);
            }
        }
        if result.success() || attempt >= total_attempts {
            return result;
        }
        if !quiet {
            eprintln!(
                "  ↻ retry {}/{} after exit {} — {}ms backoff",
                attempt,
                policy.retries,
                result.exit_code,
                RETRY_DELAY.as_millis()
            );
        }
        std::thread::sleep(RETRY_DELAY);
    }
}

#[derive(Debug, Clone)]
struct SkipDecision {
    kind: String,
    expression: Option<String>,
    reason: String,
}

fn step_key(step_id: Option<&str>, block_idx: usize) -> String {
    step_id
        .map(|s| s.to_string())
        .unwrap_or_else(|| (block_idx + 1).to_string())
}

fn capture_outputs_for_result(
    result: &mut executor::BlockResult,
    continue_on_error: bool,
) -> step_outputs::StepOutputMap {
    match step_outputs::parse_outputs(&result.stdout) {
        Ok(outputs) => outputs,
        Err(e) if continue_on_error => {
            log_warn!("warning: structured output ignored: {}", e);
            step_outputs::StepOutputMap::new()
        }
        Err(e) => {
            if !result.stderr.is_empty() {
                result.stderr.push('\n');
            }
            result
                .stderr
                .push_str(&format!("wb: structured output parse failed: {}", e));
            result.exit_code = 1;
            result.error_type = Some("output_parse_failed".to_string());
            step_outputs::StepOutputMap::new()
        }
    }
}

fn raw_outputs(outputs: &step_outputs::StepOutputMap) -> BTreeMap<String, serde_json::Value> {
    outputs
        .iter()
        .map(|(k, v)| (k.clone(), v.value.clone()))
        .collect()
}

fn conditional_skip_decision(
    when: Option<&str>,
    skip_if: Option<&str>,
    env: &std::collections::HashMap<String, String>,
) -> Option<SkipDecision> {
    let reason = parser::should_skip_block(when, skip_if, env)?;
    if reason.starts_with("when=") {
        Some(SkipDecision {
            kind: "when".to_string(),
            expression: when.map(|s| s.to_string()),
            reason,
        })
    } else {
        Some(SkipDecision {
            kind: "skip_if".to_string(),
            expression: skip_if.map(|s| s.to_string()),
            reason,
        })
    }
}

fn no_run_skip_decision() -> SkipDecision {
    SkipDecision {
        kind: "no_run".to_string(),
        expression: None,
        reason: "{no-run}".to_string(),
    }
}

/// Skip decision produced when a step falls outside the user's selection
/// range (`--only` / `--from` / `--until`). Emitted as `step.skipped` with
/// the same shape as `no_run`/`when`/`skip_if` so consumers don't need a
/// new event type.
fn selection_skip_decision() -> SkipDecision {
    SkipDecision {
        kind: "selection".to_string(),
        expression: None,
        reason: "outside --only/--from/--until range".to_string(),
    }
}

/// Skip decision produced when `--cache` has a successful entry for this block's
/// source + params (#18). Same `step.skipped` shape as the other skip kinds.
fn cache_skip_decision() -> SkipDecision {
    SkipDecision {
        kind: "cache".to_string(),
        expression: None,
        reason: "unchanged source + params since a cached success".to_string(),
    }
}

/// Skip decision produced when an operator `goto_step` jumps the cursor past
/// an executable step. Same `step.skipped` shape as the other skip kinds.
fn goto_skip_decision() -> SkipDecision {
    SkipDecision {
        kind: "goto".to_string(),
        expression: None,
        reason: "skipped by operator goto_step".to_string(),
    }
}

/// Validate that every navigation action declared on a `pause_for_human`
/// resolves to a real step id (F7b), so the run page never offers a button that
/// resolves to nothing. `rerun_step` with no target means "the current step"
/// and is always valid. Returns the first offending target id.
fn validate_pause_action_targets(
    actions: &[serde_json::Value],
    steps: &[step_ir::Step],
) -> Result<(), String> {
    for action in actions {
        let kind = action.get("kind").and_then(|k| k.as_str()).unwrap_or("");
        if kind != "rerun_step" && kind != "goto_step" {
            continue;
        }
        if let Some(target) = action.get("target").and_then(|t| t.as_str()) {
            if !steps.iter().any(|s| s.id.as_str() == target) {
                return Err(target.to_string());
            }
        }
    }
    Ok(())
}

/// In-flight navigation the operator chose at a `pause_for_human` (F7b).
/// `RerunStep(None)` re-runs the currently paused step; `RerunStep(Some(id))`
/// and `GotoStep(id)` move the cursor to `id` (earlier = re-run intervening,
/// later = skip intervening).
#[derive(Debug, Clone, PartialEq)]
enum ResumeAction {
    Resume,
    RerunStep(Option<String>),
    GotoStep(String),
}

/// Decide the resume navigation from CLI flags and the resume signal payload.
/// Precedence: CLI flag > signal `action` object > plain forward `Resume`.
fn resolve_resume_action(
    cli_rerun: &Option<Option<String>>,
    cli_goto: &Option<String>,
    signal: Option<&serde_json::Value>,
) -> ResumeAction {
    if let Some(target) = cli_goto {
        return ResumeAction::GotoStep(target.clone());
    }
    if let Some(opt) = cli_rerun {
        return ResumeAction::RerunStep(opt.clone());
    }
    if let Some(action) = signal.and_then(|v| v.get("action")) {
        let kind = action
            .get("kind")
            .and_then(|k| k.as_str())
            .unwrap_or("resume");
        let target = action
            .get("target")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());
        match kind {
            "rerun_step" => return ResumeAction::RerunStep(target),
            "goto_step" => {
                if let Some(t) = target {
                    return ResumeAction::GotoStep(t);
                }
            }
            _ => {}
        }
    }
    ResumeAction::Resume
}

/// Resolve CLI selection flags against the workbook's step list. Empty
/// selection (no flags) returns the full `0..block_count` range. An unknown
/// step id is a usage error so the user finds out before the run starts.
fn resolve_selection(
    sel: &SelectionArgs,
    steps: &[step_ir::Step],
    block_count: usize,
) -> Result<std::ops::Range<usize>, String> {
    if sel.is_empty() {
        return Ok(0..block_count);
    }
    let find = |id: &str| -> Result<usize, String> {
        steps
            .iter()
            .position(|s| s.id.as_str() == id)
            .ok_or_else(|| format!("step id '{}' not found in workbook", id))
    };
    if let Some(ref id) = sel.only {
        let pos = find(id)?;
        return Ok(pos..pos + 1);
    }
    let start = match sel.from {
        Some(ref id) => find(id)?,
        None => 0,
    };
    let end = match sel.until {
        Some(ref id) => find(id)? + 1, // --until is inclusive in user terms
        None => block_count,
    };
    if start >= end {
        return Err(format!(
            "selection range is empty: --from '{}' resolves to position {} but --until '{}' resolves to position {}",
            sel.from.as_deref().unwrap_or(""),
            start + 1,
            sel.until.as_deref().unwrap_or(""),
            if end > 0 { end } else { 1 },
        ));
    }
    Ok(start..end)
}

fn should_emit_skip_callback(silent: bool, workflow: Option<&callback::WorkflowPayload>) -> bool {
    !silent || workflow.is_some()
}

use clap::{Parser, Subcommand};
use exit::WbExit;
use output::OutputFormat;

// ─── Per-subcommand arg structs ───────────────────────────────────────────────

#[derive(clap::Args, Clone)]
struct RunArgs {
    /// Path to a markdown file or folder of workbooks
    file: String,
    #[arg(short, long)]
    output: Option<String>,
    #[arg(long, group = "format")]
    json: bool,
    #[arg(long, group = "format")]
    yaml: bool,
    #[arg(long, group = "format")]
    md: bool,
    #[arg(long)]
    secrets: Option<String>,
    #[arg(long)]
    project: Option<String>,
    #[arg(long = "secrets-cmd")]
    secrets_cmd: Option<String>,
    #[arg(short = 'C', long)]
    dir: Option<String>,
    #[arg(short, long)]
    quiet: bool,
    #[arg(short, long, hide = true)]
    verbose: bool,
    #[arg(long)]
    bail: bool,
    #[arg(long)]
    no_setup: bool,
    #[arg(long, default_value = "a-z")]
    order: String,
    #[arg(long)]
    checkpoint: Option<String>,
    #[arg(long, env = "WB_CALLBACK_URL")]
    callback: Option<String>,
    #[arg(long = "callback-secret", env = "WB_CALLBACK_SECRET")]
    callback_secret: Option<String>,
    #[arg(long = "callback-key", env = "WB_CALLBACK_KEY")]
    callback_key: Option<String>,
    #[arg(short = 'e', long = "set", value_name = "KEY=VALUE")]
    set_vars: Vec<String>,
    #[arg(long = "env-file", value_name = "PATH")]
    env_files: Vec<String>,
    #[arg(long = "env-file-relative")]
    env_file_relative: bool,
    #[arg(long)]
    redact: Vec<String>,
    /// Run only this step (by step id) and skip everything else.
    /// Conflicts with --from/--until.
    #[arg(long, value_name = "STEP_ID")]
    only: Option<String>,
    /// Start execution at this step (by step id). Earlier steps are
    /// skipped — they don't run and don't checkpoint. Combines with
    /// --until to bound the range.
    #[arg(long, value_name = "STEP_ID", conflicts_with = "only")]
    from: Option<String>,
    /// Stop execution after this step (by step id), inclusive. Later
    /// steps are skipped. Combines with --from for an explicit range.
    #[arg(long, value_name = "STEP_ID", conflicts_with = "only")]
    until: Option<String>,
    /// Run only blocks carrying this fence `.class` tag (repeatable). Composes
    /// with --from/--until; conflicts with --only.
    #[arg(long = "tag", value_name = "CLASS", conflicts_with = "only")]
    tag: Vec<String>,
    /// Run only blocks new or edited vs a git ref (default HEAD). Composes with
    /// --from/--until/--tag; conflicts with --only.
    #[arg(long = "changed", conflicts_with = "only")]
    changed: bool,
    /// Git ref that --changed diffs against (default HEAD).
    #[arg(long = "changed-base", value_name = "REF", default_value = "HEAD")]
    changed_base: String,
    /// Cap how long a block may run before wb kills it. Accepts duration
    /// strings ("30s", "5m", "2h") or bare seconds. Without this flag and
    /// without a `timeouts._default` frontmatter entry, blocks run unbounded.
    /// Per-block `timeouts: {N: ...}` entries still win over this default.
    #[arg(long = "default-block-timeout", value_name = "DURATION")]
    default_block_timeout: Option<String>,
    /// Set a declared parameter: `--param region=us-east-1`. Repeatable.
    /// Highest precedence over --param-file, --profile, and declared defaults.
    #[arg(long = "param", value_name = "KEY=VALUE")]
    param: Vec<String>,
    /// Load parameter values from a YAML file (mapping of name: value).
    #[arg(long = "param-file", value_name = "PATH")]
    param_file: Option<String>,
    /// Apply a named parameter profile declared under `profiles:`.
    #[arg(long = "profile", value_name = "NAME")]
    profile: Option<String>,
    /// Print the resolved execution plan (which blocks would run, skip, and the
    /// resolved command for each) and exit without running anything. Does not
    /// resolve secrets or run setup.
    #[arg(long = "dry-run")]
    dry_run: bool,
    /// Refuse to run unless this workbook's content is recorded as trusted
    /// (`wb trust add`). Also enabled by `$WB_REQUIRE_TRUST=1`. A trust-on-
    /// first-use integrity check — not a signature; see `wb trust`.
    #[arg(long = "require-trust")]
    require_trust: bool,
    /// Refuse to run unless the workbook carries a valid ed25519 signature
    /// (`wb sign`). Pin the author with `--pubkey`. (#37)
    #[arg(long = "verify-sig")]
    verify_sig: bool,
    /// Required signer public key (hex) for `--verify-sig`.
    #[arg(long = "pubkey", value_name = "HEX")]
    pubkey: Option<String>,
    /// Allowlist the runtimes a workbook may use (repeatable). A block whose
    /// language isn't listed makes the run refuse before any block executes —
    /// an *enforceable* policy gate (wb dispatches by language). (#37)
    #[arg(long = "allow-runtime", value_name = "LANG")]
    allow_runtime: Vec<String>,
    /// Run inside a Docker container for OS isolation even when the workbook has
    /// no `requires:` block (sandbox-by-default for untrusted code). (#37)
    #[arg(long = "sandbox")]
    sandbox: bool,
    /// Launch the `--sandbox` container with `--network none` (no network).
    #[arg(long = "sandbox-no-network")]
    sandbox_no_network: bool,
    /// Refuse to run unless the workbook's input identity matches its lockfile
    /// (see `wb lock`). Detects drift in the runbook or its included files.
    #[arg(long = "locked")]
    locked: bool,
    /// Lockfile path for `--locked` (default: <file>.lock).
    #[arg(long = "lockfile", value_name = "PATH")]
    lockfile: Option<String>,
    /// On a block failure, POST the (redacted) failure to this endpoint and
    /// apply the returned `{"action":"rerun"|"skip"|"abort"}` — self-healing
    /// runs (#42). `patch` (code injection) is intentionally not supported.
    #[arg(long = "repair", value_name = "URL")]
    repair: Option<String>,
    /// Max repair reruns per failed block (default 3) — bounds repair loops.
    #[arg(long = "repair-max", value_name = "N", default_value = "3")]
    repair_max: u32,
    /// Append each run event as a JSONL line to this file (a local event sink
    /// for `tail -f` / a viewer; works with or without `--callback`).
    #[arg(long = "events", value_name = "FILE")]
    events: Option<String>,
    /// Enable the source-hash execution cache under this id (`~/.wb/cache/<id>`):
    /// skip blocks whose source + params are unchanged since a prior success.
    #[arg(long = "cache", value_name = "ID")]
    cache: Option<String>,
    /// Disable the cache for this run even if `--cache` is given.
    #[arg(long = "no-cache")]
    no_cache: bool,
}

#[derive(clap::Args)]
struct InspectArgs {
    file: String,
    /// Emit JSON instead of human prose
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
struct ValidateArgs {
    /// File or folder to validate
    file: String,
    #[arg(long, default_value = "text")]
    format: String,
    #[arg(long = "strict")]
    strict: bool,
}

#[derive(clap::Args)]
struct TestArgs {
    /// Markdown file or folder of workbooks to test.
    file: String,
    /// Output format: text | json.
    #[arg(long, default_value = "text")]
    format: String,
    /// Stop a file at its first failing assertion (still reports it).
    #[arg(long)]
    bail: bool,
    #[arg(short, long)]
    quiet: bool,
    #[arg(short = 'C', long)]
    dir: Option<String>,
    #[arg(long)]
    secrets: Option<String>,
    #[arg(long)]
    project: Option<String>,
    #[arg(long = "secrets-cmd")]
    secrets_cmd: Option<String>,
    #[arg(long)]
    no_setup: bool,
    #[arg(short = 'e', long = "set", value_name = "KEY=VALUE")]
    set_vars: Vec<String>,
    #[arg(long = "env-file", value_name = "PATH")]
    env_files: Vec<String>,
    #[arg(long = "env-file-relative")]
    env_file_relative: bool,
    #[arg(long)]
    redact: Vec<String>,
    #[arg(long = "default-block-timeout", value_name = "DURATION")]
    default_block_timeout: Option<String>,
    #[arg(long = "param", value_name = "KEY=VALUE")]
    param: Vec<String>,
    #[arg(long = "param-file", value_name = "PATH")]
    param_file: Option<String>,
    #[arg(long = "profile", value_name = "NAME")]
    profile: Option<String>,
}

#[derive(clap::Args)]
struct ArtifactsArgs {
    #[command(subcommand)]
    sub: ArtifactsSub,
}

#[derive(Subcommand)]
enum ArtifactsSub {
    /// List artifacts recorded for a run (defaults to the most recent run).
    List {
        /// Run id (defaults to the latest run under ~/.wb/runs).
        #[arg(long)]
        run: Option<String>,
        #[command(flatten)]
        fmt: FormatArg,
    },
    /// Print the absolute path of one artifact (use in `$(wb artifacts open x)`).
    Open {
        name: String,
        #[arg(long)]
        run: Option<String>,
    },
    /// Copy an artifact out to a destination path (file or directory).
    Export {
        name: String,
        #[arg(long = "to", value_name = "DEST")]
        to: String,
        #[arg(long)]
        run: Option<String>,
    },
}

#[derive(clap::Args)]
struct RunsArgs {
    #[command(subcommand)]
    sub: RunsSub,
}

#[derive(Subcommand)]
enum RunsSub {
    /// List known runs (newest first) with artifact counts.
    List {
        #[command(flatten)]
        fmt: FormatArg,
    },
    /// Show a run's artifacts and (if present) its checkpoint state.
    Show {
        id: String,
        #[command(flatten)]
        fmt: FormatArg,
    },
}

#[derive(clap::Args)]
struct KeygenArgs {
    /// Key file prefix (writes <prefix> + <prefix>.pub). Default ~/.wb/keys/wb_signing_key.
    #[arg(long = "out", value_name = "PREFIX")]
    out: Option<String>,
    #[command(flatten)]
    fmt: FormatArg,
}

#[derive(clap::Args)]
struct SignArgs {
    /// Workbook to sign.
    file: String,
    /// Private key path (default ~/.wb/keys/wb_signing_key).
    #[arg(long = "key", value_name = "PATH")]
    key: Option<String>,
    /// Signature output path (default <file>.sig).
    #[arg(long = "out", value_name = "PATH")]
    out: Option<String>,
}

#[derive(clap::Args)]
struct VerifySigArgs {
    /// Workbook whose signature to verify.
    file: String,
    /// Signature file (default <file>.sig).
    #[arg(long = "sig", value_name = "PATH")]
    sig: Option<String>,
    /// Required signer public key (hex). Without it, only signature validity is
    /// checked, not *who* signed — pass `--pubkey` to pin the author.
    #[arg(long = "pubkey", value_name = "HEX")]
    pubkey: Option<String>,
    #[command(flatten)]
    fmt: FormatArg,
}

#[derive(clap::Args)]
struct LockArgs {
    /// Workbook to lock.
    file: String,
    /// Lockfile path (default: <file>.lock).
    #[arg(long = "lockfile", value_name = "PATH")]
    lockfile: Option<String>,
}

#[derive(clap::Args)]
struct TrustArgs {
    #[command(subcommand)]
    sub: TrustSub,
}

#[derive(Subcommand)]
enum TrustSub {
    /// Record a workbook's current content as trusted (review it first!).
    Add {
        file: String,
        #[command(flatten)]
        fmt: FormatArg,
    },
    /// Show the trust status of a workbook (trusted/untrusted/changed).
    Check {
        file: String,
        #[command(flatten)]
        fmt: FormatArg,
    },
    /// Remove a workbook from the trust store.
    Remove {
        file: String,
        #[command(flatten)]
        fmt: FormatArg,
    },
    /// List all trusted workbooks.
    List {
        #[command(flatten)]
        fmt: FormatArg,
    },
}

#[derive(clap::Args)]
struct CaptureArgs {
    /// Output workbook path (default: stdout).
    #[arg(short, long)]
    output: Option<String>,
    /// Runtime language for captured commands.
    #[arg(long, default_value = "bash")]
    runtime: String,
    /// Add an `expect` fence after each block asserting its exit code (and a
    /// stdout substring when the output is short + quote-safe).
    #[arg(long)]
    assert: bool,
    /// Title for the generated workbook.
    #[arg(long)]
    title: Option<String>,
    /// Working directory to run the captured commands in.
    #[arg(short = 'C', long)]
    dir: Option<String>,
    /// Interactive: read one command at a time, run it live (showing output),
    /// and record it — a REPL-style session recorder. End with Ctrl-D.
    #[arg(short = 'i', long)]
    interactive: bool,
}

#[derive(clap::Args)]
struct WatchArgs {
    /// Checkpoint id of the run to watch (the `--checkpoint <id>` of `wb run`).
    id: String,
    /// Print a single snapshot and exit instead of watching live.
    #[arg(long)]
    once: bool,
    /// Poll interval in seconds while watching.
    #[arg(long, default_value = "1")]
    interval: u64,
    /// Serve a local web viewer instead of the terminal view (#35). Open the
    /// printed http://127.0.0.1:<port>/ — it polls the run state as JSON.
    #[arg(long)]
    serve: bool,
    /// Port for `--serve` (default 7878).
    #[arg(long, default_value = "7878")]
    port: u16,
    #[command(flatten)]
    fmt: FormatArg,
}

#[derive(clap::Args)]
struct DoctorArgs {
    /// Run optional deep checks that probe Docker/Redis/sidecar
    #[arg(long)]
    deep: bool,
    #[arg(long, default_value = "text")]
    format: String,
}

#[derive(clap::Args)]
struct PendingArgs {
    #[arg(long, default_value = "text")]
    format: String,
    #[arg(long = "no-reap")]
    no_reap: bool,
}

#[derive(clap::Args)]
struct ResumeArgs {
    /// Checkpoint id to resume (omit to auto-detect from pending signals)
    id: Option<String>,
    /// Signal payload JSON file (use `-` for stdin)
    #[arg(long)]
    signal: Option<String>,
    /// Provide a single value for the bound var directly (shorthand for simple waits)
    #[arg(long)]
    value: Option<String>,
    #[arg(long)]
    secrets: Option<String>,
    #[arg(long)]
    project: Option<String>,
    #[arg(long = "secrets-cmd")]
    secrets_cmd: Option<String>,
    #[arg(short = 'C', long)]
    dir: Option<String>,
    #[arg(short, long)]
    quiet: bool,
    #[arg(long)]
    bail: bool,
    #[arg(long)]
    no_setup: bool,
    #[arg(long, env = "WB_CALLBACK_URL")]
    callback: Option<String>,
    #[arg(long = "callback-secret", env = "WB_CALLBACK_SECRET")]
    callback_secret: Option<String>,
    #[arg(long = "callback-key", env = "WB_CALLBACK_KEY")]
    callback_key: Option<String>,
    #[arg(short = 'e', long = "set", value_name = "KEY=VALUE")]
    set_vars: Vec<String>,
    #[arg(long = "env-file", value_name = "PATH")]
    env_files: Vec<String>,
    #[arg(long = "env-file-relative")]
    env_file_relative: bool,
    #[arg(long)]
    redact: Vec<String>,
    #[arg(short, long)]
    output: Option<String>,
    #[arg(long, group = "format")]
    json: bool,
    #[arg(long, group = "format")]
    yaml: bool,
    #[arg(long, group = "format")]
    md: bool,
    /// Re-run a step from its first verb instead of resuming forward. With no
    /// value, re-runs the currently paused step (the "run now" button). Takes a
    /// step id to re-run from an earlier step. Mutually exclusive with
    /// --goto-step.
    #[arg(long = "rerun-step", value_name = "STEP_ID", num_args = 0..=1, group = "nav")]
    rerun_step: Option<Option<String>>,
    /// Jump the execution cursor to STEP_ID before resuming: an earlier id
    /// re-runs the intervening steps, a later id skips them (emitting
    /// step.skipped). Mutually exclusive with --rerun-step.
    #[arg(long = "goto-step", value_name = "STEP_ID", group = "nav")]
    goto_step: Option<String>,
}

#[derive(clap::Args)]
struct CancelArgs {
    id: String,
    #[command(flatten)]
    fmt: FormatArg,
}

#[derive(clap::Args)]
struct ContainersArgs {
    #[command(subcommand)]
    sub: ContainersSub,
}

#[derive(Subcommand)]
enum ContainersSub {
    Build {
        path: Option<String>,
        #[command(flatten)]
        fmt: FormatArg,
    },
    #[command(alias = "ls")]
    List {
        #[command(flatten)]
        fmt: FormatArg,
    },
    Prune {
        #[command(flatten)]
        fmt: FormatArg,
    },
}

#[derive(clap::Args)]
struct UpdateArgs {
    #[arg(long)]
    check: bool,
}

#[derive(clap::Args)]
struct TransformArgs {
    file: String,
}

/// Reusable `--format text|json` flag for management commands. Flattened into
/// each subcommand so it parses after the subcommand token
/// (`wb config list --format json`), matching the `validate`/`doctor`/`pending`
/// convention.
#[derive(clap::Args, Clone)]
struct FormatArg {
    /// Output format: text | json
    #[arg(long, default_value = "text")]
    format: String,
}

#[derive(clap::Args)]
struct ConfigArgs {
    #[command(subcommand)]
    sub: ConfigSub,
}

#[derive(Subcommand)]
enum ConfigSub {
    /// List all set config values (and the known keys you can set).
    List {
        #[command(flatten)]
        fmt: FormatArg,
    },
    /// Print the value of a single key.
    Get {
        key: String,
        #[command(flatten)]
        fmt: FormatArg,
    },
    /// Set a key to a value (key must be one of the known keys).
    Set {
        key: String,
        value: String,
        #[command(flatten)]
        fmt: FormatArg,
    },
    /// Remove a key.
    Unset {
        key: String,
        #[command(flatten)]
        fmt: FormatArg,
    },
    /// Print the path to the config file.
    Path {
        #[command(flatten)]
        fmt: FormatArg,
    },
}

#[derive(Subcommand)]
enum Command {
    // Boxed: RunArgs is much larger than the other variants (clippy
    // large_enum_variant) now that it carries the full run-flag surface.
    Run(Box<RunArgs>),
    Inspect(InspectArgs),
    Validate(ValidateArgs),
    /// Run a workbook (or folder) and evaluate its `expect`/`assert` fences.
    Test(TestArgs),
    /// Docs-as-tests: run a doc's code blocks and fail if any block errors (or
    /// any `expect` assertion fails). Like `wb test` but assertions are optional.
    Verify(TestArgs),
    /// Inspect artifacts captured by runs (list / open / export).
    Artifacts(ArtifactsArgs),
    /// Inspect past runs and their artifacts/checkpoint state.
    Runs(RunsArgs),
    /// Watch a checkpointed run live (progress, results, pending waits).
    Watch(WatchArgs),
    /// Capture a sequence of commands (from stdin) into a runnable workbook.
    Capture(CaptureArgs),
    /// Manage the trust-on-first-use store (`wb run --require-trust`).
    Trust(TrustArgs),
    /// Write a reproducibility lockfile of a workbook's input identity (#47).
    Lock(LockArgs),
    /// Generate an ed25519 signing keypair (#37).
    Keygen(KeygenArgs),
    /// Sign a workbook, writing a detached `<file>.sig` (#37/#40).
    Sign(SignArgs),
    /// Verify a workbook's signature (#37).
    #[command(name = "verify-sig")]
    VerifySig(VerifySigArgs),
    Doctor(DoctorArgs),
    Pending(PendingArgs),
    Resume(ResumeArgs),
    Cancel(CancelArgs),
    Containers(ContainersArgs),
    Config(ConfigArgs),
    Update(UpdateArgs),
    Version(FormatArg),
    /// Hidden — frontmatter scaffolding helper, kept for backwards compat.
    #[command(hide = true)]
    Transform(TransformArgs),
    /// Print a shell completion script to stdout (bash, zsh, fish, …).
    Completion {
        /// Target shell. Source the output, e.g. `wb completion zsh > _wb`.
        shell: clap_complete::Shell,
    },
    /// Print a man page (roff) for `wb` to stdout.
    Man,
    /// Run a Model Context Protocol server over stdio (for MCP clients/agents).
    Mcp,
}

/// Bare-run args: the flags that work when the user types `wb file.md` without
/// a subcommand. Mirrors RunArgs plus `--inspect` for backward compatibility.
#[derive(clap::Args)]
struct BareRunArgs {
    file: Option<String>,
    #[arg(short, long)]
    output: Option<String>,
    #[arg(long, group = "format")]
    json: bool,
    #[arg(long, group = "format")]
    yaml: bool,
    #[arg(long, group = "format")]
    md: bool,
    #[arg(long)]
    secrets: Option<String>,
    #[arg(long)]
    project: Option<String>,
    #[arg(long = "secrets-cmd")]
    secrets_cmd: Option<String>,
    #[arg(short = 'C', long)]
    dir: Option<String>,
    #[arg(short, long)]
    quiet: bool,
    #[arg(short, long, hide = true)]
    verbose: bool,
    #[arg(long)]
    bail: bool,
    #[arg(long)]
    no_setup: bool,
    /// Inspect workbook structure without executing (kept for backward compat)
    #[arg(short, long, hide = true)]
    inspect: bool,
    #[arg(long, default_value = "a-z")]
    order: String,
    #[arg(long)]
    checkpoint: Option<String>,
    #[arg(long, env = "WB_CALLBACK_URL")]
    callback: Option<String>,
    #[arg(long = "callback-secret", env = "WB_CALLBACK_SECRET")]
    callback_secret: Option<String>,
    #[arg(long = "callback-key", env = "WB_CALLBACK_KEY")]
    callback_key: Option<String>,
    #[arg(short = 'e', long = "set", value_name = "KEY=VALUE")]
    set_vars: Vec<String>,
    #[arg(long = "env-file", value_name = "PATH")]
    env_files: Vec<String>,
    #[arg(long = "env-file-relative")]
    env_file_relative: bool,
    #[arg(long)]
    redact: Vec<String>,
    #[arg(long, value_name = "STEP_ID")]
    only: Option<String>,
    #[arg(long, value_name = "STEP_ID", conflicts_with = "only")]
    from: Option<String>,
    #[arg(long, value_name = "STEP_ID", conflicts_with = "only")]
    until: Option<String>,
    #[arg(long = "tag", value_name = "CLASS", conflicts_with = "only")]
    tag: Vec<String>,
    #[arg(long = "changed", conflicts_with = "only")]
    changed: bool,
    #[arg(long = "changed-base", value_name = "REF", default_value = "HEAD")]
    changed_base: String,
    #[arg(long = "default-block-timeout", value_name = "DURATION")]
    default_block_timeout: Option<String>,
    #[arg(long = "param", value_name = "KEY=VALUE")]
    param: Vec<String>,
    #[arg(long = "param-file", value_name = "PATH")]
    param_file: Option<String>,
    #[arg(long = "profile", value_name = "NAME")]
    profile: Option<String>,
    #[arg(long = "dry-run")]
    dry_run: bool,
    #[arg(long = "require-trust")]
    require_trust: bool,
    #[arg(long = "verify-sig")]
    verify_sig: bool,
    #[arg(long = "pubkey", value_name = "HEX")]
    pubkey: Option<String>,
    #[arg(long = "allow-runtime", value_name = "LANG")]
    allow_runtime: Vec<String>,
    #[arg(long = "sandbox")]
    sandbox: bool,
    #[arg(long = "sandbox-no-network")]
    sandbox_no_network: bool,
    #[arg(long = "locked")]
    locked: bool,
    #[arg(long = "lockfile", value_name = "PATH")]
    lockfile: Option<String>,
    #[arg(long = "repair", value_name = "URL")]
    repair: Option<String>,
    #[arg(long = "repair-max", value_name = "N", default_value = "3")]
    repair_max: u32,
    #[arg(long = "events", value_name = "FILE")]
    events: Option<String>,
    #[arg(long = "cache", value_name = "ID")]
    cache: Option<String>,
    #[arg(long = "no-cache")]
    no_cache: bool,
}

#[derive(Parser)]
#[command(name = "wb", version, about = "Run markdown workbooks")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Stderr verbosity: error | warn | info | debug (default info). Lower it to
    /// `error` to silence checkpoint/outputs/upload warnings in CI/agents.
    #[arg(
        long = "log-level",
        global = true,
        env = "WB_LOG_LEVEL",
        default_value = "info",
        value_name = "LEVEL"
    )]
    log_level: String,

    #[command(flatten)]
    bare_run: BareRunArgs,
}

/// Library entry point — the `wb` binary is a thin shim over this. Also lets
/// the parser/IR/diagnostic core be embedded (e.g. a WASM preview build).
pub fn run() -> std::process::ExitCode {
    let cli = Cli::parse();
    match logging::parse_level(&cli.log_level) {
        Ok(l) => logging::set_level(l),
        Err(e) => {
            eprintln!("wb: {e}");
            return std::process::ExitCode::from(2u8);
        }
    }
    let exit = match cli.command {
        Some(Command::Run(args)) => cmd_run(*args),
        Some(Command::Inspect(args)) => cmd_inspect(args),
        Some(Command::Validate(args)) => cmd_validate(args),
        Some(Command::Test(args)) => cmd_test(args),
        Some(Command::Verify(args)) => cmd_verify(args),
        Some(Command::Artifacts(args)) => cmd_artifacts(args),
        Some(Command::Runs(args)) => cmd_runs(args),
        Some(Command::Watch(args)) => cmd_watch(args),
        Some(Command::Capture(args)) => cmd_capture(args),
        Some(Command::Trust(args)) => cmd_trust(args),
        Some(Command::Lock(args)) => cmd_lock(args),
        Some(Command::Keygen(args)) => cmd_keygen(args),
        Some(Command::Sign(args)) => cmd_sign(args),
        Some(Command::VerifySig(args)) => cmd_verify_sig(args),
        Some(Command::Doctor(args)) => cmd_doctor(args),
        Some(Command::Pending(args)) => cmd_pending_cmd(args),
        Some(Command::Resume(args)) => cmd_resume_cmd(args),
        Some(Command::Cancel(args)) => cmd_cancel_cmd(args),
        Some(Command::Containers(args)) => cmd_containers_cmd(args),
        Some(Command::Config(args)) => cmd_config(args),
        Some(Command::Update(args)) => {
            update::cmd_update(args.check);
            WbExit::Success
        }
        Some(Command::Version(fmt)) => cmd_version(&fmt),
        Some(Command::Transform(args)) => {
            transform_workbook(&args.file);
            WbExit::Success
        }
        Some(Command::Completion { shell }) => cmd_completion(shell),
        Some(Command::Man) => cmd_man(),
        Some(Command::Mcp) => mcp::run(),
        None => {
            let mut bare = cli.bare_run;
            let Some(file) = bare.file.take() else {
                print_short_usage();
                return std::process::ExitCode::from(2u8);
            };
            if bare.inspect {
                cmd_inspect(InspectArgs {
                    file,
                    json: bare.json,
                })
            } else {
                cmd_run(RunArgs {
                    file,
                    output: bare.output,
                    json: bare.json,
                    yaml: bare.yaml,
                    md: bare.md,
                    secrets: bare.secrets,
                    project: bare.project,
                    secrets_cmd: bare.secrets_cmd,
                    dir: bare.dir,
                    quiet: bare.quiet,
                    verbose: bare.verbose,
                    bail: bare.bail,
                    no_setup: bare.no_setup,
                    order: bare.order,
                    checkpoint: bare.checkpoint,
                    callback: bare.callback,
                    callback_secret: bare.callback_secret,
                    callback_key: bare.callback_key,
                    set_vars: bare.set_vars,
                    env_files: bare.env_files,
                    env_file_relative: bare.env_file_relative,
                    redact: bare.redact,
                    only: bare.only,
                    from: bare.from,
                    until: bare.until,
                    tag: bare.tag,
                    changed: bare.changed,
                    changed_base: bare.changed_base,
                    default_block_timeout: bare.default_block_timeout,
                    param: bare.param,
                    param_file: bare.param_file,
                    profile: bare.profile,
                    dry_run: bare.dry_run,
                    require_trust: bare.require_trust,
                    verify_sig: bare.verify_sig,
                    pubkey: bare.pubkey,
                    allow_runtime: bare.allow_runtime,
                    sandbox: bare.sandbox,
                    sandbox_no_network: bare.sandbox_no_network,
                    locked: bare.locked,
                    lockfile: bare.lockfile,
                    repair: bare.repair,
                    repair_max: bare.repair_max,
                    events: bare.events,
                    cache: bare.cache,
                    no_cache: bare.no_cache,
                })
            }
        }
    };
    if let Some(msg) = exit.message() {
        eprintln!("wb: {}", msg);
    }
    std::process::ExitCode::from(exit.code() as u8)
}

fn print_short_usage() {
    eprintln!("usage: wb <file.md>");
    eprintln!("       wb run <folder/> -o report.json");
    eprintln!("       wb <file.md> --json");
    eprintln!("       wb update");
}

/// The action an agent endpoint returns for a failed block (#42). `patch`
/// `patch` carries a replacement command the endpoint wants run in place of the
/// failed block. It is real code execution — but `--repair` is an explicit
/// opt-in to an endpoint the operator chose, which already controls the run
/// (rerun/skip/abort), and the workbook itself is arbitrary code. wb logs the
/// patched command prominently so it's never silent.
enum RepairAction {
    Rerun,
    Skip,
    Abort,
    Patch(String),
}

/// POST a failed block to the `--repair` endpoint and parse the returned
/// `{"action": "rerun"|"skip"|"abort"|"patch", "code": "…"}`. Secret values are
/// redacted from the block output before it leaves the box. Any network/parse
/// error → `Abort` (fail safe: don't silently skip, patch, or loop).
fn repair_consult(
    url: &str,
    result: &executor::BlockResult,
    block_idx: usize,
    language: &str,
    session: &executor::Session,
) -> RepairAction {
    let redact = session.redact_values();
    let payload = serde_json::json!({
        "event": "block.failed",
        "block_index": block_idx,
        "language": language,
        "exit_code": result.exit_code,
        "error_type": result.error_type,
        "stdout": executor::redact_output(&result.stdout, redact),
        "stderr": executor::redact_output(&result.stderr, redact),
    })
    .to_string();

    let out = std::process::Command::new("curl")
        .args(["-sS", "--max-time", "30", "-X", "POST", "-H"])
        .arg("Content-Type: application/json")
        .arg("-d")
        .arg(&payload)
        .arg(url)
        .output();
    let body = match out {
        Ok(o) if o.status.success() => o.stdout,
        Ok(_) | Err(_) => {
            log_warn!("warning: --repair endpoint unreachable; aborting");
            return RepairAction::Abort;
        }
    };
    let parsed: serde_json::Value =
        serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    match parsed.get("action").and_then(|a| a.as_str()) {
        Some("rerun") => RepairAction::Rerun,
        Some("skip") => RepairAction::Skip,
        Some("patch") => match parsed.get("code").and_then(|c| c.as_str()) {
            Some(code) if !code.trim().is_empty() => RepairAction::Patch(code.to_string()),
            _ => {
                log_warn!("warning: --repair patch action missing `code`; aborting");
                RepairAction::Abort
            }
        },
        _ => RepairAction::Abort,
    }
}

/// Resolve a remote workbook ref to an `https://` URL, or `None` for a local
/// path. `gh:OWNER/REPO/PATH[@REF]` → raw.githubusercontent.com; bare
/// `http(s)://…` URLs pass through. (#40, the safe remote-fetch piece.)
fn resolve_remote_url(arg: &str) -> Option<String> {
    if let Some(rest) = arg.strip_prefix("gh:") {
        let (spec, git_ref) = match rest.split_once('@') {
            Some((s, r)) => (s, r),
            None => (rest, "HEAD"),
        };
        let mut parts = spec.splitn(3, '/');
        let owner = parts.next().filter(|s| !s.is_empty())?;
        let repo = parts.next().filter(|s| !s.is_empty())?;
        let path = parts.next().filter(|s| !s.is_empty())?;
        return Some(format!(
            "https://raw.githubusercontent.com/{owner}/{repo}/{git_ref}/{path}"
        ));
    }
    if arg.starts_with("http://") || arg.starts_with("https://") {
        return Some(arg.to_string());
    }
    None
}

/// Fetch a remote workbook to `~/.wb/remote/<sha-of-url>.md` and return the
/// local cache path. The content is downloaded but never executed here —
/// `cmd_run` always trust-gates remote workbooks (TOFU).
fn fetch_remote(url: &str) -> Result<std::path::PathBuf, String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    let key: String = hasher
        .finalize()
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect();
    let dir = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(".wb")
        .join("remote");
    std::fs::create_dir_all(&dir).map_err(|e| format!("remote cache: {e}"))?;
    let dest = dir.join(format!("{key}.md"));
    let out = std::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "30", "-o"])
        .arg(&dest)
        .arg(url)
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "failed to fetch {url}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(dest)
}

fn cmd_run(mut args: RunArgs) -> WbExit {
    // Remote ref (#40): fetch gh:/https: workbooks to a local cache. Remote
    // workbooks are ALWAYS trust-gated (TOFU) and never auto-executed.
    let mut remote_forced_trust = false;
    if let Some(url) = resolve_remote_url(&args.file) {
        match fetch_remote(&url) {
            Ok(path) => {
                eprintln!("wb: fetched {url}\n    → {}", path.display());
                args.file = path.to_string_lossy().into_owned();
                remote_forced_trust = true;
            }
            Err(e) => {
                eprintln!("error: {e}");
                return WbExit::Usage(e);
            }
        }
    }
    cmd_run_inner(args, remote_forced_trust)
}

fn cmd_run_inner(args: RunArgs, remote_forced_trust: bool) -> WbExit {
    let format_flag = if args.json {
        Some(OutputFormat::Json)
    } else if args.yaml {
        Some(OutputFormat::Yaml)
    } else if args.md {
        Some(OutputFormat::Markdown)
    } else {
        None
    };

    let file_format = args.output.as_deref().and_then(OutputFormat::from_path);
    let output_format = format_flag.or(file_format);
    let stdout_output = format_flag.is_some() && args.output.is_none();

    let cli_vars: std::collections::HashMap<String, String> = args
        .set_vars
        .iter()
        .filter_map(|s| {
            s.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect();

    let cli_default_timeout = match args.default_block_timeout.as_deref() {
        Some(s) => match parser::parse_duration_secs(s) {
            Ok(n) => Some(Duration::from_secs(n)),
            Err(e) => {
                eprintln!("error: --default-block-timeout: {}", e);
                return WbExit::Usage("invalid --default-block-timeout".to_string());
            }
        },
        None => None,
    };

    let p = Path::new(&args.file);

    // Trust-on-first-use gate (#37): refuse to run an untrusted/changed
    // workbook when --require-trust (or $WB_REQUIRE_TRUST=1) is set.
    let require_trust = args.require_trust
        || remote_forced_trust
        || std::env::var("WB_REQUIRE_TRUST").ok().as_deref() == Some("1");
    if require_trust {
        if p.is_dir() {
            eprintln!("error: --require-trust is not supported for folder runs yet; run files individually");
            return WbExit::Usage("require-trust unsupported for folders".to_string());
        }
        match trust::TrustStore::load().status(&args.file) {
            trust::TrustStatus::Trusted => {}
            status => {
                eprintln!(
                    "error: refusing to run {} — workbook is {}. Review it, then run `wb trust add {}`.",
                    args.file,
                    status.label(),
                    args.file
                );
                return WbExit::Usage(format!("untrusted workbook ({})", status.label()));
            }
        }
    }

    // Runtime allowlist (#37): an enforceable policy — refuse before any block
    // runs if the workbook uses a language not on the allowlist.
    if !args.allow_runtime.is_empty() && !p.is_dir() {
        if let Ok(content) = std::fs::read_to_string(&args.file) {
            let allowed: std::collections::HashSet<String> = args
                .allow_runtime
                .iter()
                .map(|s| s.to_lowercase())
                .collect();
            let offenders: Vec<String> = parse_and_resolve(&content, &args.file)
                .build_steps()
                .iter()
                .map(|s| s.language.to_lowercase())
                .filter(|l| !allowed.contains(l))
                .collect();
            if let Some(bad) = offenders.first() {
                eprintln!(
                    "error: refusing to run {} — runtime '{}' is not in the --allow-runtime allowlist [{}].",
                    args.file,
                    bad,
                    args.allow_runtime.join(", ")
                );
                return WbExit::Usage(format!("disallowed runtime '{bad}'"));
            }
        }
    }

    // Signature gate (#37): refuse to run unless the workbook carries a valid
    // ed25519 signature (and matches --pubkey when pinned).
    if args.verify_sig {
        if p.is_dir() {
            eprintln!("error: --verify-sig is not supported for folder runs yet");
            return WbExit::Usage("verify-sig unsupported for folders".to_string());
        }
        let content = match std::fs::read(&args.file) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("error: {}: {}", args.file, e);
                return WbExit::Io(e.to_string());
            }
        };
        let sp = signing::sig_path(&args.file, None);
        match signing::load_sig(&sp)
            .and_then(|sig| signing::verify(&sig, &content, args.pubkey.as_deref()))
        {
            Ok(()) => {
                if !args.quiet {
                    eprintln!("wb: signature OK for {}", args.file);
                }
            }
            Err(e) => {
                eprintln!(
                    "error: refusing to run {} — signature check failed: {e}. Sign it with `wb sign {}`.",
                    args.file, args.file
                );
                return WbExit::Usage("signature verification failed".to_string());
            }
        }
    }

    // Reproducibility lockfile gate (#47): refuse to run if the workbook's
    // input identity drifted from its lockfile.
    if args.locked {
        if p.is_dir() {
            eprintln!("error: --locked is not supported for folder runs yet");
            return WbExit::Usage("locked unsupported for folders".to_string());
        }
        let lp = lockfile::lock_path(&args.file, args.lockfile.as_deref());
        match (std::fs::read_to_string(&args.file), lockfile::load(&lp)) {
            (Ok(content), Ok(lock)) => {
                let steps = parse_and_resolve(&content, &args.file).build_steps();
                if let Err(drift) = lockfile::verify(&lock, &steps) {
                    eprintln!(
                        "error: --locked: {drift}. If the change is intended, re-run `wb lock {}`.",
                        args.file
                    );
                    return WbExit::Usage("workbook drifted from lockfile".to_string());
                }
            }
            (_, Err(e)) => {
                eprintln!("error: --locked: {e}. Run `wb lock {}` first.", args.file);
                return WbExit::Usage("missing or invalid lockfile".to_string());
            }
            (Err(e), _) => {
                eprintln!("error: {}: {}", args.file, e);
                return WbExit::Io(e.to_string());
            }
        }
    }

    if p.is_dir() {
        run_folder(
            &args.file,
            args.output,
            output_format,
            stdout_output,
            args.secrets,
            args.project,
            args.secrets_cmd,
            args.dir,
            args.quiet,
            args.bail,
            &args.order,
            args.no_setup,
            cli_vars,
            args.redact,
            args.env_files,
            args.env_file_relative,
            cli_default_timeout,
        );
    } else {
        run_single(RunConfig {
            file: args.file,
            output_path: args.output,
            output_format,
            stdout_output,
            secrets_override: args.secrets.clone(),
            project: args.project.clone(),
            secrets_cmd: args.secrets_cmd.clone(),
            dir: args.dir,
            quiet: args.quiet,
            bail: args.bail,
            no_setup: args.no_setup,
            checkpoint_id: args.checkpoint,
            callback_url: args.callback,
            callback_secret: args.callback_secret,
            callback_key: args.callback_key,
            cli_vars,
            cli_redact: args.redact,
            env_files: args.env_files,
            env_file_relative: args.env_file_relative,
            selection: SelectionArgs {
                only: args.only,
                from: args.from,
                until: args.until,
                tag: args.tag,
                changed: args.changed,
                changed_base: args.changed_base,
            },
            default_block_timeout: cli_default_timeout,
            browser_restart: false,
            skipped_by_goto: std::collections::HashSet::new(),
            param_inputs: args.param,
            param_file: args.param_file,
            profile: args.profile,
            dry_run: args.dry_run,
            cache_id: if args.no_cache { None } else { args.cache },
            repair_url: args.repair,
            repair_max: args.repair_max,
            events_path: args.events,
            sandbox: args.sandbox,
            sandbox_no_network: args.sandbox_no_network,
        });
    }
    WbExit::Success
}

/// Resolve a run's artifacts directory from an optional run id. With an id,
/// `~/.wb/runs/<id>/artifacts`. Without one, the most recent run. `None` if no
/// runs exist or the named run has no artifacts dir.
fn resolve_run_dir(run: Option<&str>) -> Option<(String, std::path::PathBuf)> {
    match run {
        Some(id) => {
            let dir = artifacts::run_artifacts_dir(id);
            Some((id.to_string(), dir))
        }
        None => artifacts::list_runs().into_iter().next(),
    }
}

/// Build a manifest for a run dir — preferring the persisted `manifest.json`,
/// falling back to a live scan of the directory (no step provenance).
fn manifest_for(run_id: &str, dir: &std::path::Path) -> artifacts::Manifest {
    if let Some(m) = artifacts::load_manifest(dir) {
        return m;
    }
    // Fallback: scan the directory directly so artifacts written before the
    // manifest feature (or by external tooling) still list.
    let mut entries = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_file() {
                continue;
            }
            let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if name.ends_with(".meta.json")
                || name.ends_with(".wb.json")
                || name == artifacts::MANIFEST_FILENAME
                || name == "pause_result.json"
            {
                continue;
            }
            entries.push(artifacts::ManifestEntry {
                filename: name.to_string(),
                bytes: e.metadata().map(|m| m.len()).unwrap_or(0),
                content_type: "application/octet-stream".to_string(),
                sha256: artifacts::sha256_file(&p).unwrap_or_default(),
                label: None,
                description: None,
                step_id: None,
                updated_at: String::new(),
            });
        }
    }
    entries.sort_by(|a, b| a.filename.cmp(&b.filename));
    artifacts::Manifest {
        run_id: run_id.to_string(),
        artifacts: entries,
    }
}

fn cmd_artifacts(args: ArtifactsArgs) -> WbExit {
    match args.sub {
        ArtifactsSub::List { run, fmt } => {
            let json = match want_json(&fmt.format) {
                Ok(b) => b,
                Err(e) => return e,
            };
            let Some((run_id, dir)) = resolve_run_dir(run.as_deref()) else {
                eprintln!("wb: no runs found under ~/.wb/runs");
                return WbExit::Usage("no runs found".to_string());
            };
            let manifest = manifest_for(&run_id, &dir);
            if json {
                print_json(&serde_json::json!({
                    "run_id": manifest.run_id,
                    "dir": dir.to_string_lossy(),
                    "artifacts": manifest.artifacts.iter().map(|e| serde_json::json!({
                        "filename": e.filename,
                        "bytes": e.bytes,
                        "content_type": e.content_type,
                        "sha256": e.sha256,
                        "label": e.label,
                        "description": e.description,
                        "step_id": e.step_id,
                        "updated_at": e.updated_at,
                    })).collect::<Vec<_>>(),
                }));
            } else {
                println!("{} ({})", output::style_bold(&run_id), dir.display());
                if manifest.artifacts.is_empty() {
                    println!("  (no artifacts)");
                }
                for e in &manifest.artifacts {
                    let label = e
                        .label
                        .as_deref()
                        .map(|l| format!("  {l}"))
                        .unwrap_or_default();
                    let step = e
                        .step_id
                        .as_deref()
                        .map(|s| format!(" #{s}"))
                        .unwrap_or_default();
                    println!(
                        "  {} ({}, {} B){}{}",
                        e.filename, e.content_type, e.bytes, step, label
                    );
                }
            }
            WbExit::Success
        }
        ArtifactsSub::Open { name, run } => {
            let Some((_run_id, dir)) = resolve_run_dir(run.as_deref()) else {
                eprintln!("wb: no runs found under ~/.wb/runs");
                return WbExit::Usage("no runs found".to_string());
            };
            let path = dir.join(&name);
            if !path.is_file() {
                eprintln!("wb: artifact '{}' not found in {}", name, dir.display());
                return WbExit::BlockFailed;
            }
            println!("{}", path.display());
            WbExit::Success
        }
        ArtifactsSub::Export { name, to, run } => {
            let Some((_run_id, dir)) = resolve_run_dir(run.as_deref()) else {
                eprintln!("wb: no runs found under ~/.wb/runs");
                return WbExit::Usage("no runs found".to_string());
            };
            let src = dir.join(&name);
            if !src.is_file() {
                eprintln!("wb: artifact '{}' not found in {}", name, dir.display());
                return WbExit::BlockFailed;
            }
            let dest = std::path::Path::new(&to);
            let dest = if dest.is_dir() {
                dest.join(&name)
            } else {
                dest.to_path_buf()
            };
            match std::fs::copy(&src, &dest) {
                Ok(_) => {
                    eprintln!("wb: exported {} → {}", name, dest.display());
                    WbExit::Success
                }
                Err(e) => {
                    eprintln!("error: export {}: {}", name, e);
                    WbExit::BlockFailed
                }
            }
        }
    }
}

fn cmd_runs(args: RunsArgs) -> WbExit {
    match args.sub {
        RunsSub::List { fmt } => {
            let json = match want_json(&fmt.format) {
                Ok(b) => b,
                Err(e) => return e,
            };
            let runs = artifacts::list_runs();
            if json {
                let arr: Vec<serde_json::Value> = runs
                    .iter()
                    .map(|(id, dir)| {
                        let count = manifest_for(id, dir).artifacts.len();
                        serde_json::json!({ "run_id": id, "dir": dir.to_string_lossy(), "artifacts": count })
                    })
                    .collect();
                print_json(&serde_json::json!({ "runs": arr }));
            } else if runs.is_empty() {
                println!("(no runs under ~/.wb/runs)");
            } else {
                for (id, dir) in &runs {
                    let count = manifest_for(id, dir).artifacts.len();
                    println!("{}  {} artifact(s)", output::style_bold(id), count);
                }
            }
            WbExit::Success
        }
        RunsSub::Show { id, fmt } => {
            let json = match want_json(&fmt.format) {
                Ok(b) => b,
                Err(e) => return e,
            };
            let dir = artifacts::run_artifacts_dir(&id);
            let manifest = manifest_for(&id, &dir);
            // Surface checkpoint state too, if a checkpoint shares this id.
            let ckpt = checkpoint::load(&id).ok().flatten();
            if json {
                print_json(&serde_json::json!({
                    "run_id": id,
                    "dir": dir.to_string_lossy(),
                    "exists": dir.exists(),
                    "artifacts": manifest.artifacts.len(),
                    "checkpoint": ckpt.as_ref().map(|c| serde_json::json!({
                        "status": format!("{:?}", c.status),
                        "next_block": c.next_block,
                        "total_blocks": c.total_blocks,
                    })),
                }));
            } else {
                println!("{}", output::style_bold(&format!("run {id}")));
                println!("  artifacts dir: {}", dir.display());
                println!("  artifacts: {}", manifest.artifacts.len());
                for e in &manifest.artifacts {
                    println!("    - {} ({} B)", e.filename, e.bytes);
                }
                match ckpt {
                    Some(c) => println!(
                        "  checkpoint: {:?}, next block {}/{}",
                        c.status,
                        c.next_block + 1,
                        c.total_blocks
                    ),
                    None => println!("  checkpoint: none"),
                }
            }
            WbExit::Success
        }
    }
}

/// `wb capture` (#41) — read a sequence of shell commands from stdin, run each
/// in a real session, and emit a runnable workbook with each command as a block
/// (optionally followed by an `expect` fence asserting the observed exit code +
/// a stdout substring). One command per line; `#`-prefixed lines become
/// Markdown headings. Turns an ad-hoc session into a checked-in, re-runnable
/// runbook. PTY/interactive recording is a future extension.
/// Process one captured line: prose for `# …`, else run the command in the
/// session and append the block (+ optional `expect`) to `md`. Returns whether
/// a command was executed (so the caller can count).
fn capture_one_line(
    session: &mut executor::Session,
    md: &mut String,
    idx: usize,
    line: &str,
    runtime: &str,
    assert: bool,
) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    if let Some(comment) = trimmed.strip_prefix("# ") {
        md.push_str(&format!("## {comment}\n\n"));
        return false;
    }
    let block = parser::CodeBlock {
        language: runtime.to_string(),
        code: line.to_string(),
        line_number: 0,
        skip_execution: false,
        silent: false,
        when: None,
        skip_if: None,
        no_cache: false,
        attrs: Default::default(),
    };
    let result = session.execute_block(&block, idx);
    md.push_str(&format!("```{runtime}\n{line}\n```\n\n"));
    if assert {
        let mut asserts = format!("exit {}\n", result.exit_code);
        if let Some(sub) = capture_stdout_assertion(&result.stdout) {
            asserts.push_str(&format!("stdout contains \"{sub}\"\n"));
        }
        md.push_str(&format!("```expect\n{asserts}```\n\n"));
    }
    true
}

fn cmd_capture(args: CaptureArgs) -> WbExit {
    use std::io::{BufRead, Read, Write};

    // Build a minimal execution session for the chosen runtime. Interactive
    // mode shows command output live; batch mode stays quiet.
    let fm = parser::Frontmatter {
        runtime: Some(args.runtime.clone()),
        ..Default::default()
    };
    let mut ctx = executor::ExecutionContext::from_frontmatter(&fm, "capture");
    if let Some(ref d) = args.dir {
        ctx.working_dir = d.clone();
    }
    ctx.quiet = !args.interactive;
    let mut session = executor::Session::new(ctx);

    let mut md = String::new();
    md.push_str("---\n");
    if let Some(ref t) = args.title {
        md.push_str(&format!("title: {t}\n"));
    }
    md.push_str(&format!("runtime: {}\n", args.runtime));
    md.push_str("---\n\n");
    if let Some(ref t) = args.title {
        md.push_str(&format!("# {t}\n\n"));
    }

    let mut idx = 0usize;
    if args.interactive {
        // REPL: prompt, read a line, run it live, record — until Ctrl-D.
        eprintln!("wb capture (interactive) — type commands, Ctrl-D to finish.");
        let stdin = std::io::stdin();
        loop {
            eprint!("wb» ");
            let _ = std::io::stderr().flush();
            let mut line = String::new();
            match stdin.lock().read_line(&mut line) {
                Ok(0) => break, // EOF (Ctrl-D)
                Ok(_) => {
                    let line = line.trim_end_matches(['\n', '\r']);
                    if capture_one_line(
                        &mut session,
                        &mut md,
                        idx,
                        line,
                        &args.runtime,
                        args.assert,
                    ) {
                        idx += 1;
                    }
                }
                Err(_) => break,
            }
        }
        eprintln!();
    } else {
        let mut input = String::new();
        if std::io::stdin().read_to_string(&mut input).is_err() {
            eprintln!("error: capture reads commands from stdin (or pass --interactive)");
            return WbExit::Usage("capture: no stdin".to_string());
        }
        for raw in input.lines() {
            let line = raw.trim_end();
            if capture_one_line(&mut session, &mut md, idx, line, &args.runtime, args.assert) {
                idx += 1;
            }
        }
    }

    if idx == 0 {
        eprintln!("wb capture: no commands on stdin");
        return WbExit::Usage("capture: empty input".to_string());
    }

    match args.output {
        Some(path) => match std::fs::write(&path, &md) {
            Ok(_) => {
                eprintln!("wb: captured {idx} command(s) → {path}");
                WbExit::Success
            }
            Err(e) => {
                eprintln!("error: write {path}: {e}");
                WbExit::Io(e.to_string())
            }
        },
        None => {
            print!("{md}");
            WbExit::Success
        }
    }
}

/// Pick a stable, quote-safe stdout substring for a capture assertion, or
/// `None` when the output isn't a good fit (empty, multi-token noise, quotes).
fn capture_stdout_assertion(stdout: &str) -> Option<String> {
    let line = stdout.lines().find(|l| !l.trim().is_empty())?.trim();
    if line.is_empty() || line.len() > 60 || line.contains('"') || line.contains('\\') {
        return None;
    }
    Some(line.to_string())
}

/// `wb lock <file>` — write a reproducibility lockfile of the workbook's input
/// identity (a sha256 per resolved step, includes expanded) to `<file>.lock`
/// (#47). Commit it; `wb run --locked` then fails on drift.
fn cmd_lock(args: LockArgs) -> WbExit {
    let content = match std::fs::read_to_string(&args.file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}: {}", args.file, e);
            return WbExit::Io(e.to_string());
        }
    };
    let steps = parse_and_resolve(&content, &args.file).build_steps();
    let lock = lockfile::build(&args.file, &steps);
    let path = lockfile::lock_path(&args.file, args.lockfile.as_deref());
    match lockfile::save(&path, &lock) {
        Ok(_) => {
            eprintln!("wb: wrote {} ({} steps)", path.display(), lock.steps.len());
            WbExit::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            WbExit::Io(e)
        }
    }
}

/// `wb keygen` — generate an ed25519 signing keypair (#37).
fn cmd_keygen(args: KeygenArgs) -> WbExit {
    let json = match want_json(&args.fmt.format) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let prefix = args
        .out
        .map(std::path::PathBuf::from)
        .unwrap_or_else(signing::default_key_path);
    match signing::keygen(&prefix) {
        Ok(pubhex) => {
            if json {
                print_json(&serde_json::json!({
                    "key": prefix.to_string_lossy(),
                    "pubkey_file": format!("{}.pub", prefix.display()),
                    "pubkey": pubhex,
                }));
            } else {
                eprintln!("wb: wrote private key {} (mode 0600)", prefix.display());
                eprintln!("wb: public key: {pubhex}");
            }
            WbExit::Success
        }
        Err(e) => {
            eprintln!("error: keygen: {e}");
            WbExit::Io(e)
        }
    }
}

/// `wb sign <file>` — write a detached `<file>.sig` (#37/#40).
fn cmd_sign(args: SignArgs) -> WbExit {
    let key = args
        .key
        .map(std::path::PathBuf::from)
        .unwrap_or_else(signing::default_key_path);
    let content = match std::fs::read(&args.file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}: {}", args.file, e);
            return WbExit::Io(e.to_string());
        }
    };
    match signing::sign(&key, &content) {
        Ok(sig) => {
            let out = signing::sig_path(&args.file, args.out.as_deref());
            if let Err(e) = signing::save_sig(&out, &sig) {
                eprintln!("error: {e}");
                return WbExit::Io(e);
            }
            eprintln!(
                "wb: signed {} → {} (key {})",
                args.file,
                out.display(),
                sig.pubkey
            );
            WbExit::Success
        }
        Err(e) => {
            eprintln!("error: sign: {e}");
            WbExit::Usage(e)
        }
    }
}

/// `wb verify-sig <file>` — verify a workbook's signature (#37).
fn cmd_verify_sig(args: VerifySigArgs) -> WbExit {
    let json = match want_json(&args.fmt.format) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let content = match std::fs::read(&args.file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}: {}", args.file, e);
            return WbExit::Io(e.to_string());
        }
    };
    let sp = signing::sig_path(&args.file, args.sig.as_deref());
    let result = signing::load_sig(&sp)
        .and_then(|sig| signing::verify(&sig, &content, args.pubkey.as_deref()));
    match result {
        Ok(()) => {
            if json {
                print_json(&serde_json::json!({"file": args.file, "ok": true}));
            } else {
                println!("{}: signature OK", args.file);
            }
            WbExit::Success
        }
        Err(e) => {
            if json {
                print_json(&serde_json::json!({"file": args.file, "ok": false, "error": e}));
            } else {
                eprintln!("{}: {e}", args.file);
            }
            WbExit::Usage("signature verification failed".to_string())
        }
    }
}

/// `wb trust` — manage the trust-on-first-use store (#37). `add` records a
/// reviewed workbook's hash; `check` reports its status; `remove`/`list` manage
/// the store. Honest scope: an integrity check (detects changes to a known
/// workbook), not a signature — see `src/trust.rs`.
fn cmd_trust(args: TrustArgs) -> WbExit {
    match args.sub {
        TrustSub::Add { file, fmt } => {
            let json = match want_json(&fmt.format) {
                Ok(b) => b,
                Err(e) => return e,
            };
            let mut store = trust::TrustStore::load();
            match store.trust(&file) {
                Ok(hash) => {
                    if let Err(e) = store.save() {
                        eprintln!("error: trust store: {e}");
                        return WbExit::Io(e.to_string());
                    }
                    if json {
                        print_json(&serde_json::json!({"ok": true, "file": file, "sha256": hash}));
                    } else {
                        eprintln!("wb: trusted {file}");
                    }
                    WbExit::Success
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    WbExit::Usage(e)
                }
            }
        }
        TrustSub::Check { file, fmt } => {
            let json = match want_json(&fmt.format) {
                Ok(b) => b,
                Err(e) => return e,
            };
            let status = trust::TrustStore::load().status(&file);
            if json {
                print_json(&serde_json::json!({"file": file, "status": status.label()}));
            } else {
                println!("{}: {}", file, status.label());
            }
            // Non-trusted is a non-zero (usage) exit so scripts can gate on it.
            if status == trust::TrustStatus::Trusted {
                WbExit::Success
            } else {
                WbExit::Usage(status.label().to_string())
            }
        }
        TrustSub::Remove { file, fmt } => {
            let json = match want_json(&fmt.format) {
                Ok(b) => b,
                Err(e) => return e,
            };
            let mut store = trust::TrustStore::load();
            let removed = store.remove(&file);
            if removed {
                if let Err(e) = store.save() {
                    eprintln!("error: trust store: {e}");
                    return WbExit::Io(e.to_string());
                }
            }
            if json {
                print_json(&serde_json::json!({"ok": removed, "file": file}));
            } else if removed {
                eprintln!("wb: removed {file} from trust store");
            } else {
                eprintln!("wb: {file} was not in the trust store");
            }
            WbExit::Success
        }
        TrustSub::List { fmt } => {
            let json = match want_json(&fmt.format) {
                Ok(b) => b,
                Err(e) => return e,
            };
            let store = trust::TrustStore::load();
            if json {
                print_json(&serde_json::json!({
                    "entries": store.entries.iter().map(|(k, v)| serde_json::json!({"file": k, "sha256": v})).collect::<Vec<_>>()
                }));
            } else if store.entries.is_empty() {
                println!("(no trusted workbooks)");
            } else {
                for (k, v) in &store.entries {
                    println!("{}  {}", &v[..v.len().min(12)], k);
                }
            }
            WbExit::Success
        }
    }
}

/// `wb watch <id>` — a local run viewer (#35/#44). Polls the checkpoint +
/// pending descriptor for a checkpointed run and renders progress, per-block
/// results, and any pending wait. Watches until the run reaches a terminal
/// state (complete/failed) or `--once`/`--format json` requests a single
/// snapshot. No new dependency — a clear-screen redraw over the existing,
/// already-standardized checkpoint state.
/// Minimal local web viewer for a checkpointed run (#35) — a dependency-free
/// `std::net` HTTP server. `GET /` serves an HTML page that polls `GET /state`
/// (the same JSON as `wb watch --format json`) and renders progress + results.
/// Loopback-only; serves one request per connection.
fn watch_serve(id: &str, port: u16) -> WbExit {
    use std::io::{Read, Write};
    let listener = match std::net::TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: cannot bind 127.0.0.1:{port}: {e}");
            return WbExit::Usage(format!("bind failed: {e}"));
        }
    };
    eprintln!("wb: serving run viewer for '{id}' at http://127.0.0.1:{port}/  (Ctrl-C to stop)");

    const PAGE: &str = r#"<!doctype html><html><head><meta charset=utf-8>
<title>wb watch</title><style>
body{font:14px/1.5 ui-monospace,Menlo,monospace;margin:2rem;color:#222}
h1{font-size:1.1rem}.ok{color:#157f3b}.bad{color:#c0392b}.dim{color:#888}
.bar{height:8px;background:#eee;border-radius:4px;overflow:hidden;margin:.5rem 0}
.bar>div{height:100%;background:#157f3b}li{margin:.1rem 0}</style></head>
<body><h1 id=t>wb watch</h1><div class=bar><div id=p style=width:0></div></div>
<div id=s class=dim></div><ul id=r></ul>
<script>
async function tick(){try{let d=await (await fetch('/state')).json();
document.getElementById('t').textContent='wb watch — '+d.checkpoint+' ['+d.status+']';
let pct=d.total_blocks?Math.round(100*d.next_block/d.total_blocks):0;
document.getElementById('p').style.width=pct+'%';
document.getElementById('s').textContent=d.workbook+' — block '+d.next_block+'/'+d.total_blocks+'  ('+d.passed+' ok, '+d.failed+' failed, '+d.skipped+' skipped)';
document.getElementById('r').innerHTML=(d.results||[]).map(r=>'<li><span class="'+(r.exit_code===0?'ok':'bad')+'">'+(r.exit_code===0?'✓':'✗')+'</span> ['+(r.block_index+1)+'] '+r.language+' <span class=dim>'+(r.heading||'')+'</span></li>').join('');
}catch(e){document.getElementById('s').textContent='(run finished or no checkpoint)';}}
tick();setInterval(tick,1000);
</script></body></html>"#;

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };
        let mut buf = [0u8; 1024];
        let n = stream.read(&mut buf).unwrap_or(0);
        let req = String::from_utf8_lossy(&buf[..n]);
        let path = req
            .split_whitespace()
            .nth(1)
            .unwrap_or("/")
            .split('?')
            .next()
            .unwrap_or("/");
        let (status, ctype, body) = if path == "/state" {
            let snap = match checkpoint::load(id).ok().flatten() {
                Some(ckpt) => {
                    let pending = pending::load(id).ok().flatten();
                    watch_snapshot_json(id, &ckpt, pending.as_ref()).to_string()
                }
                None => serde_json::json!({"checkpoint": id, "status": "gone"}).to_string(),
            };
            ("200 OK", "application/json", snap)
        } else if path == "/" {
            ("200 OK", "text/html; charset=utf-8", PAGE.to_string())
        } else {
            ("404 Not Found", "text/plain", "not found".to_string())
        };
        let resp = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(resp.as_bytes());
    }
    WbExit::Success
}

fn cmd_watch(args: WatchArgs) -> WbExit {
    if args.serve {
        return watch_serve(&args.id, args.port);
    }
    let json = match want_json(&args.fmt.format) {
        Ok(b) => b,
        Err(e) => return e,
    };

    // JSON or --once: a single snapshot.
    if json || args.once {
        let Some(ckpt) = checkpoint::load(&args.id).ok().flatten() else {
            eprintln!("wb: no checkpoint '{}' to watch", args.id);
            return WbExit::Usage(format!("no checkpoint '{}'", args.id));
        };
        let pending = pending::load(&args.id).ok().flatten();
        if json {
            print_json(&watch_snapshot_json(&args.id, &ckpt, pending.as_ref()));
        } else {
            print!("{}", render_watch(&args.id, &ckpt, pending.as_ref()));
        }
        return WbExit::Success;
    }

    // Live: redraw until terminal. Ctrl-C exits.
    let mut first = true;
    loop {
        let ckpt = checkpoint::load(&args.id).ok().flatten();
        match ckpt {
            None => {
                if first {
                    eprintln!("wb: no checkpoint '{}' to watch", args.id);
                    return WbExit::Usage(format!("no checkpoint '{}'", args.id));
                }
                // The checkpoint vanished mid-watch (completed + cleaned up).
                break;
            }
            Some(ckpt) => {
                let pending = pending::load(&args.id).ok().flatten();
                // Clear screen + home cursor for a stable live view.
                print!(
                    "\x1b[2J\x1b[H{}",
                    render_watch(&args.id, &ckpt, pending.as_ref())
                );
                use std::io::Write;
                let _ = std::io::stdout().flush();
                if matches!(
                    ckpt.status,
                    checkpoint::CheckpointStatus::Complete | checkpoint::CheckpointStatus::Failed
                ) {
                    break;
                }
            }
        }
        first = false;
        std::thread::sleep(Duration::from_secs(args.interval.max(1)));
    }
    WbExit::Success
}

fn watch_snapshot_json(
    id: &str,
    ckpt: &checkpoint::Checkpoint,
    pending: Option<&pending::PendingDescriptor>,
) -> serde_json::Value {
    let passed = ckpt.results.iter().filter(|r| r.exit_code == 0).count();
    let failed = ckpt.results.iter().filter(|r| r.exit_code != 0).count();
    serde_json::json!({
        "checkpoint": id,
        "workbook": ckpt.workbook,
        "status": format!("{:?}", ckpt.status).to_lowercase(),
        "next_block": ckpt.next_block,
        "total_blocks": ckpt.total_blocks,
        "passed": passed,
        "failed": failed,
        "skipped": ckpt.skipped.len(),
        "results": ckpt.results.iter().map(|r| serde_json::json!({
            "block_index": r.block_index,
            "step_id": r.step_id,
            "language": r.language,
            "exit_code": r.exit_code,
            "line": r.line_number,
            "heading": r.heading,
        })).collect::<Vec<_>>(),
        "pending": pending.map(|p| serde_json::json!({
            "kind": p.kind,
            "line": p.line_number,
            "message": p.message,
            "timeout_at": p.timeout_at,
        })),
    })
}

fn render_watch(
    id: &str,
    ckpt: &checkpoint::Checkpoint,
    pending: Option<&pending::PendingDescriptor>,
) -> String {
    let passed = ckpt.results.iter().filter(|r| r.exit_code == 0).count();
    let failed = ckpt.results.iter().filter(|r| r.exit_code != 0).count();
    let mut out = String::new();
    out.push_str(&format!(
        "{} [{}]\n",
        output::style_bold(&format!("watch {id}")),
        format!("{:?}", ckpt.status).to_lowercase()
    ));
    out.push_str(&format!("  {}\n", ckpt.workbook));
    out.push_str(&format!(
        "  progress: block {}/{}  ({} ok, {} failed, {} skipped)\n",
        ckpt.next_block.min(ckpt.total_blocks),
        ckpt.total_blocks,
        passed,
        failed,
        ckpt.skipped.len()
    ));
    for r in &ckpt.results {
        let mark = if r.exit_code == 0 {
            output::style_ok("✓")
        } else {
            output::style_fail("✗")
        };
        let head = r.heading.as_deref().unwrap_or("");
        out.push_str(&format!(
            "    {} [{}] {} (L{}) {}\n",
            mark,
            r.block_index + 1,
            r.language,
            r.line_number,
            head
        ));
    }
    if let Some(p) = pending {
        out.push_str(&format!(
            "  {} waiting at L{}{}{}\n",
            output::style_dim("⏸"),
            p.line_number,
            p.kind
                .as_deref()
                .map(|k| format!(" ({k})"))
                .unwrap_or_default(),
            p.message
                .as_deref()
                .map(|m| format!(" — {m}"))
                .unwrap_or_default(),
        ));
    }
    out
}

fn cmd_inspect(args: InspectArgs) -> WbExit {
    if args.json {
        inspect_workbook_json(&args.file);
    } else {
        inspect_workbook(&args.file);
    }
    WbExit::Success
}

fn cmd_validate(args: ValidateArgs) -> WbExit {
    let opts = validate::ValidateOptions {
        strict: args.strict,
    };

    let path = std::path::Path::new(&args.file);
    let diags = if path.is_dir() {
        validate::validate_dir(path, &opts)
    } else {
        validate::validate_file(path, &opts)
    };

    let output = if args.format == "json" {
        diagnostic::render_json(&diags)
    } else {
        diagnostic::render_text(&diags)
    };

    if !output.is_empty() {
        print!("{output}");
    }

    let code = validate::exit_code_for(&diags, args.strict);
    if code == exit_codes::EXIT_SUCCESS {
        WbExit::Success
    } else {
        WbExit::WorkbookInvalid(String::new())
    }
}

/// One evaluated assertion within a tested file.
struct AssertionReport {
    source: String,
    ok: bool,
    detail: String,
}

/// Aggregate result of testing a single workbook file.
struct FileReport {
    file: String,
    passed: usize,
    failed: usize,
    assertions: Vec<AssertionReport>,
    /// Set when the file couldn't be run at all (read/param error).
    error: Option<String>,
}

/// `wb test` — run a workbook (or folder) and evaluate its `expect`/`assert`
/// fences against block results. Exit 0 if all assertions pass, 1 if any fail
/// (or a file errors), 2 if no assertions were found or on a usage error.
fn cmd_test(args: TestArgs) -> WbExit {
    let json = match want_json(&args.format) {
        Ok(b) => b,
        Err(e) => return e,
    };

    let path = Path::new(&args.file);
    let files: Vec<std::path::PathBuf> = if path.is_dir() {
        let mut v: Vec<std::path::PathBuf> = match std::fs::read_dir(path) {
            Ok(rd) => rd
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
                .collect(),
            Err(e) => {
                eprintln!("error: {}: {}", args.file, e);
                return WbExit::Usage(format!("cannot read directory {}", args.file));
            }
        };
        v.sort();
        v
    } else {
        vec![path.to_path_buf()]
    };

    let reports: Vec<FileReport> = files
        .iter()
        .map(|f| test_one_file(&f.to_string_lossy(), &args))
        .collect();

    let total_passed: usize = reports.iter().map(|r| r.passed).sum();
    let total_failed: usize = reports.iter().map(|r| r.failed).sum();
    let total_assertions = total_passed + total_failed;
    let any_error = reports.iter().any(|r| r.error.is_some());

    if json {
        let files_json: Vec<serde_json::Value> = reports
            .iter()
            .map(|r| {
                serde_json::json!({
                    "file": r.file,
                    "passed": r.passed,
                    "failed": r.failed,
                    "error": r.error,
                    "assertions": r.assertions.iter().map(|a| serde_json::json!({
                        "source": a.source,
                        "ok": a.ok,
                        "detail": a.detail,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        print_json(&serde_json::json!({
            "ok": total_failed == 0 && !any_error,
            "passed": total_passed,
            "failed": total_failed,
            "files": files_json,
        }));
    } else {
        for r in &reports {
            println!("{}", output::style_bold(&r.file));
            if let Some(ref e) = r.error {
                println!("  {} {}", output::style_fail("error:"), e);
                continue;
            }
            for a in &r.assertions {
                if a.ok {
                    println!("  {} {}", output::style_ok("✓"), a.source);
                } else {
                    println!("  {} {} — {}", output::style_fail("✗"), a.source, a.detail);
                }
            }
            println!("  {} passed, {} failed", r.passed, r.failed);
        }
        println!(
            "\ntest: {} passed, {} failed across {} file(s)",
            total_passed,
            total_failed,
            reports.len()
        );
    }

    if total_assertions == 0 && !any_error {
        eprintln!(
            "wb test: no expect/assert fences found in {} — nothing to assert",
            args.file
        );
        return WbExit::Usage("no assertions found".to_string());
    }
    if total_failed > 0 || any_error {
        WbExit::BlockFailed
    } else {
        WbExit::Success
    }
}

/// Run one workbook and evaluate its assertion fences.
fn test_one_file(file: &str, args: &TestArgs) -> FileReport {
    let mut report = FileReport {
        file: file.to_string(),
        passed: 0,
        failed: 0,
        assertions: Vec::new(),
        error: None,
    };

    let content = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(e) => {
            report.error = Some(format!("read: {e}"));
            return report;
        }
    };
    let workbook = parse_and_resolve(&content, file);

    let resolved = match params::resolve(
        workbook.frontmatter.params.as_ref(),
        workbook.frontmatter.profiles.as_ref(),
        args.profile.as_deref(),
        args.param_file.as_deref(),
        &args.param,
    ) {
        Ok(r) => r,
        Err(e) => {
            report.error = Some(e);
            return report;
        }
    };

    let cli_default_timeout = match args.default_block_timeout.as_deref() {
        Some(s) => match parser::parse_duration_secs(s) {
            Ok(n) => Some(Duration::from_secs(n)),
            Err(e) => {
                report.error = Some(format!("--default-block-timeout: {e}"));
                return report;
            }
        },
        None => None,
    };

    let cli_vars: std::collections::HashMap<String, String> = args
        .set_vars
        .iter()
        .filter_map(|s| {
            s.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect();

    let summary = run_single_collect(
        file,
        args.secrets.clone(),
        args.project.clone(),
        args.secrets_cmd.clone(),
        args.dir.clone(),
        args.quiet,
        args.no_setup,
        cli_vars,
        args.redact.clone(),
        args.env_files.clone(),
        args.env_file_relative,
        cli_default_timeout,
        resolved.values.into_iter().collect(),
    );

    let (assertions, passed, failed) = walk_expects(&workbook, &summary, args.bail);
    report.assertions = assertions;
    report.passed = passed;
    report.failed = failed;
    report
}

/// `wb verify` — docs-as-tests (#43). Runs a doc's code blocks and fails if any
/// block errors or any `expect` assertion fails. Unlike `wb test`, assertions
/// are optional: a plain doc with only runnable code blocks passes when they all
/// exit 0. Exit 0 if every file passes, 1 otherwise.
fn cmd_verify(args: TestArgs) -> WbExit {
    let json = match want_json(&args.format) {
        Ok(b) => b,
        Err(e) => return e,
    };

    let path = Path::new(&args.file);
    let files: Vec<std::path::PathBuf> = if path.is_dir() {
        let mut v: Vec<std::path::PathBuf> = match std::fs::read_dir(path) {
            Ok(rd) => rd
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
                .collect(),
            Err(e) => {
                eprintln!("error: {}: {}", args.file, e);
                return WbExit::Usage(format!("cannot read directory {}", args.file));
            }
        };
        v.sort();
        v
    } else {
        vec![path.to_path_buf()]
    };

    let reports: Vec<FileReport> = files
        .iter()
        .map(|f| verify_one_file(&f.to_string_lossy(), &args))
        .collect();

    let total_failed: usize = reports.iter().map(|r| r.failed).sum();
    let any_error = reports.iter().any(|r| r.error.is_some());

    if json {
        let files_json: Vec<serde_json::Value> = reports
            .iter()
            .map(|r| {
                serde_json::json!({
                    "file": r.file,
                    "ok": r.failed == 0 && r.error.is_none(),
                    "checks": r.passed,
                    "failures": r.failed,
                    "error": r.error,
                    "details": r.assertions.iter().map(|a| serde_json::json!({
                        "source": a.source, "ok": a.ok, "detail": a.detail,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        print_json(&serde_json::json!({
            "ok": total_failed == 0 && !any_error,
            "files": files_json,
        }));
    } else {
        for r in &reports {
            let ok = r.failed == 0 && r.error.is_none();
            let mark = if ok {
                output::style_ok("ok  ")
            } else {
                output::style_fail("FAIL")
            };
            println!("{} {}", mark, r.file);
            if let Some(ref e) = r.error {
                println!("     {}", e);
            }
            for a in r.assertions.iter().filter(|a| !a.ok) {
                println!("     {} — {}", a.source, a.detail);
            }
        }
        let ok_files = reports
            .iter()
            .filter(|r| r.failed == 0 && r.error.is_none())
            .count();
        println!("\nverify: {}/{} file(s) ok", ok_files, reports.len());
    }

    if total_failed > 0 || any_error {
        WbExit::BlockFailed
    } else {
        WbExit::Success
    }
}

/// Verify one doc: run it, count failed blocks + failed assertions.
fn verify_one_file(file: &str, args: &TestArgs) -> FileReport {
    let mut report = FileReport {
        file: file.to_string(),
        passed: 0,
        failed: 0,
        assertions: Vec::new(),
        error: None,
    };

    let content = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(e) => {
            report.error = Some(format!("read: {e}"));
            return report;
        }
    };
    let workbook = parse_and_resolve(&content, file);

    let resolved = match params::resolve(
        workbook.frontmatter.params.as_ref(),
        workbook.frontmatter.profiles.as_ref(),
        args.profile.as_deref(),
        args.param_file.as_deref(),
        &args.param,
    ) {
        Ok(r) => r,
        Err(e) => {
            report.error = Some(e);
            return report;
        }
    };

    let cli_default_timeout = match args.default_block_timeout.as_deref() {
        Some(s) => match parser::parse_duration_secs(s) {
            Ok(n) => Some(Duration::from_secs(n)),
            Err(e) => {
                report.error = Some(format!("--default-block-timeout: {e}"));
                return report;
            }
        },
        None => None,
    };

    let cli_vars: std::collections::HashMap<String, String> = args
        .set_vars
        .iter()
        .filter_map(|s| {
            s.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect();

    let summary = run_single_collect(
        file,
        args.secrets.clone(),
        args.project.clone(),
        args.secrets_cmd.clone(),
        args.dir.clone(),
        args.quiet,
        args.no_setup,
        cli_vars,
        args.redact.clone(),
        args.env_files.clone(),
        args.env_file_relative,
        cli_default_timeout,
        resolved.values.into_iter().collect(),
    );

    // Every failed block is a verify failure (docs-as-tests: code must run).
    for r in summary.results.iter().filter(|r| !r.success()) {
        report.failed += 1;
        report.assertions.push(AssertionReport {
            source: format!("block {} ({})", r.block_index + 1, r.language),
            ok: false,
            detail: format!("exited {}", r.exit_code),
        });
    }
    // Plus any expect-fence assertions.
    let (asserts, passed, failed) = walk_expects(&workbook, &summary, args.bail);
    report.passed += passed + summary.results.iter().filter(|r| r.success()).count();
    report.failed += failed;
    report
        .assertions
        .extend(asserts.into_iter().filter(|a| !a.ok));
    report
}

/// Evaluate every `expect`/`assert` fence in a workbook against a run's
/// results. Returns the per-assertion reports plus pass/fail counts. Shared by
/// `wb test` and `wb verify`.
fn walk_expects(
    workbook: &parser::Workbook,
    summary: &output::RunSummary,
    bail: bool,
) -> (Vec<AssertionReport>, usize, usize) {
    let by_index: std::collections::HashMap<usize, &executor::BlockResult> =
        summary.results.iter().map(|r| (r.block_index, r)).collect();
    let mut reports = Vec::new();
    let mut passed = 0usize;
    let mut failed = 0usize;

    // Walk sections, tracking the block index exactly as run_single_collect
    // assigns it (Code/Browser/Wait each consume one index). An `expect` fence
    // asserts against the immediately preceding executable block.
    let mut block_counter = 0usize;
    'outer: for section in &workbook.sections {
        match section {
            parser::Section::Code(_) | parser::Section::Browser(_) | parser::Section::Wait(_) => {
                block_counter += 1
            }
            parser::Section::Expect(spec) => {
                for err in &spec.errors {
                    failed += 1;
                    reports.push(AssertionReport {
                        source: err.clone(),
                        ok: false,
                        detail: "malformed assertion".to_string(),
                    });
                    if bail {
                        break 'outer;
                    }
                }
                if block_counter == 0 {
                    for (src, _) in &spec.assertions {
                        failed += 1;
                        reports.push(AssertionReport {
                            source: src.clone(),
                            ok: false,
                            detail: "no preceding block to assert against".to_string(),
                        });
                    }
                    continue;
                }
                let target = block_counter - 1;
                match by_index.get(&target) {
                    Some(res) => {
                        for (src, a) in &spec.assertions {
                            let o = assertion::evaluate(
                                src,
                                a,
                                res.exit_code,
                                &res.stdout,
                                &res.stderr,
                            );
                            if o.ok {
                                passed += 1;
                            } else {
                                failed += 1;
                            }
                            let ok = o.ok;
                            reports.push(AssertionReport {
                                source: o.source,
                                ok,
                                detail: o.detail,
                            });
                            if !ok && bail {
                                break 'outer;
                            }
                        }
                    }
                    None => {
                        for (src, _) in &spec.assertions {
                            failed += 1;
                            reports.push(AssertionReport {
                                source: src.clone(),
                                ok: false,
                                detail: "preceding block did not run (skipped)".to_string(),
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
    (reports, passed, failed)
}

fn cmd_doctor(args: DoctorArgs) -> WbExit {
    let opts = doctor::DoctorOptions {
        deep: args.deep,
        json: args.format == "json",
    };
    let (results, code) = doctor::run(&opts);
    let output = if opts.json {
        doctor::render_json(&results)
    } else {
        doctor::render_text(&results)
    };
    print!("{output}");
    if code == exit_codes::EXIT_SUCCESS {
        WbExit::Success
    } else {
        WbExit::WorkbookInvalid(String::new())
    }
}

fn cmd_pending_cmd(args: PendingArgs) -> WbExit {
    let json_out = args.format == "json";
    cmd_pending_impl(json_out, args.no_reap);
    WbExit::Success
}

fn cmd_cancel_cmd(args: CancelArgs) -> WbExit {
    let json = match want_json(&args.fmt.format) {
        Ok(j) => j,
        Err(e) => return e,
    };
    cmd_cancel(&args.id, json)
}

fn cmd_containers_cmd(args: ContainersArgs) -> WbExit {
    match args.sub {
        ContainersSub::Build { path, fmt } => {
            let json = match want_json(&fmt.format) {
                Ok(j) => j,
                Err(e) => return e,
            };
            cmd_containers_build(path.as_deref().unwrap_or("."), json)
        }
        ContainersSub::List { fmt } => {
            let json = match want_json(&fmt.format) {
                Ok(j) => j,
                Err(e) => return e,
            };
            cmd_containers_list(json);
            WbExit::Success
        }
        ContainersSub::Prune { fmt } => {
            let json = match want_json(&fmt.format) {
                Ok(j) => j,
                Err(e) => return e,
            };
            cmd_containers_prune(json);
            WbExit::Success
        }
    }
}

/// Collect .md files from a directory, sorted by --order
fn collect_workbooks(dir: &str, order: &str) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: {}: {}", dir, e);
            std::process::exit(1);
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "md" || ext == "markdown" {
                    files.push(path.to_string_lossy().to_string());
                }
            }
        }
    }

    match order {
        "z-a" | "za" | "desc" => files.sort_by(|a, b| b.cmp(a)),
        _ => files.sort(), // a-z default
    }

    files
}

#[allow(clippy::too_many_arguments)]
fn run_folder(
    dir: &str,
    output_path: Option<String>,
    output_format: Option<OutputFormat>,
    stdout_output: bool,
    secrets_override: Option<String>,
    project: Option<String>,
    secrets_cmd: Option<String>,
    working_dir: Option<String>,
    quiet: bool,
    bail: bool,
    order: &str,
    no_setup: bool,
    cli_vars: std::collections::HashMap<String, String>,
    cli_redact: Vec<String>,
    env_files: Vec<String>,
    env_file_relative: bool,
    cli_default_block_timeout: Option<Duration>,
) {
    let files = collect_workbooks(dir, order);

    if files.is_empty() {
        eprintln!("no .md files found in {}", dir);
        std::process::exit(0);
    }

    eprintln!("wb: running {} workbooks from {}", files.len(), dir);

    let start = Instant::now();
    let mut all_summaries: Vec<output::RunSummary> = Vec::new();
    let mut total_failed = 0;

    for file in &files {
        let filename = Path::new(file)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| file.clone());

        let summary = run_single_collect(
            file,
            secrets_override.clone(),
            project.clone(),
            secrets_cmd.clone(),
            working_dir.clone(),
            quiet,
            no_setup,
            cli_vars.clone(),
            cli_redact.clone(),
            env_files.clone(),
            env_file_relative,
            cli_default_block_timeout,
            std::collections::HashMap::new(),
        );

        let line = format!(
            "  {} {} ({}/{} blocks, {:.1}s)",
            if summary.failed == 0 { "✓" } else { "✗" },
            filename,
            summary.passed,
            summary.total_blocks,
            summary.total_duration.as_secs_f64()
        );
        if summary.failed == 0 {
            eprintln!("{}", output::style_ok(&line));
        } else {
            eprintln!("{}", output::style_fail(&line));
        }

        if summary.failed > 0 {
            total_failed += 1;
        }

        all_summaries.push(summary);

        if bail && total_failed > 0 {
            eprintln!("  stopping (--bail)");
            break;
        }
    }

    let total_duration = start.elapsed();
    let total_workbooks = all_summaries.len();
    let passed_workbooks = total_workbooks - total_failed;

    eprintln!();
    if total_failed == 0 {
        eprintln!(
            "{}",
            output::style_ok(&format!(
                "✓ {} workbooks in {:.1}s",
                passed_workbooks,
                total_duration.as_secs_f64()
            ))
        );
    } else {
        eprintln!(
            "{}",
            output::style_fail(&format!(
                "✗ {} passed, {} failed in {:.1}s",
                passed_workbooks,
                total_failed,
                total_duration.as_secs_f64()
            ))
        );
    }

    // Output combined report
    if let Some(fmt) = output_format {
        let rendered = output::format_batch_output(&all_summaries, dir, total_duration, fmt);

        if stdout_output {
            println!("{}", rendered);
        }

        if let Some(ref path) = output_path {
            match std::fs::write(path, &rendered) {
                Ok(_) => eprintln!("  -> {}", path),
                Err(e) => eprintln!("error: write {}: {}", path, e),
            }
        }
    }

    if total_failed > 0 {
        std::process::exit(exit_codes::EXIT_BLOCK_FAILED);
    }
}

/// Run a single workbook and return its summary (no output/printing)
#[allow(clippy::too_many_arguments)]
fn run_single_collect(
    file: &str,
    secrets_override: Option<String>,
    project: Option<String>,
    secrets_cmd: Option<String>,
    dir: Option<String>,
    quiet: bool,
    no_setup: bool,
    cli_vars: std::collections::HashMap<String, String>,
    cli_redact: Vec<String>,
    env_files: Vec<String>,
    env_file_relative: bool,
    cli_default_block_timeout: Option<Duration>,
    extra_env: std::collections::HashMap<String, String>,
) -> output::RunSummary {
    // Resolve the trace-correlation id once so every subsequent artifact,
    // callback, and early-return RunSummary carries the same value.
    // Uses WB_RECORDING_RUN_ID → TRIGGER_RUN_ID → generated.
    let run_id = artifacts::resolve_run_id(&std::collections::HashMap::new());

    let content = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(e) => {
            return output::RunSummary {
                source_file: file.to_string(),
                run_id: run_id.clone(),
                total_blocks: 0,
                passed: 0,
                failed: 1,
                total_duration: std::time::Duration::ZERO,
                results: vec![executor::BlockResult {
                    block_index: 0,
                    language: "error".to_string(),
                    stdout: String::new(),
                    stderr: format!("read error: {}", e),
                    exit_code: 1,
                    duration: std::time::Duration::ZERO,
                    error_type: Some("read_error".to_string()),
                    stdout_partial: false,
                    stderr_partial: false,
                }],
            };
        }
    };

    let workbook = parse_and_resolve(&content, file);
    let block_count = workbook.code_block_count();

    // Sandbox: if `requires` is present and we're not inside a container,
    // build the image and re-invoke wb inside Docker. If Docker is missing
    // or the build fails, `sandbox::build_image` surfaces a clear error and
    // the caller exits with EXIT_SANDBOX_UNAVAILABLE.
    if let Some(ref requires) = workbook.frontmatter.requires {
        if std::env::var("WB_SANDBOX_INNER").ok().as_deref() != Some("1") {
            let workbook_dir = std::path::Path::new(file)
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string());

            let tag = match sandbox::build_image(requires, &workbook_dir) {
                Ok(t) => t,
                Err(e) => {
                    return output::RunSummary {
                        source_file: file.to_string(),
                        run_id: run_id.clone(),
                        total_blocks: block_count,
                        passed: 0,
                        failed: 1,
                        total_duration: std::time::Duration::ZERO,
                        results: vec![executor::BlockResult {
                            block_index: 0,
                            language: "sandbox".to_string(),
                            stdout: String::new(),
                            stderr: format!("sandbox: {}", e),
                            exit_code: 1,
                            duration: std::time::Duration::ZERO,
                            error_type: Some("sandbox_failed".to_string()),
                            stdout_partial: false,
                            stderr_partial: false,
                        }],
                    };
                }
            };

            let mut container_env: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            if let Some(ref fm_env) = workbook.frontmatter.env {
                container_env.extend(fm_env.clone());
            }
            let secrets_config = build_secrets_config(
                &workbook.frontmatter.secrets,
                secrets_override,
                project,
                secrets_cmd,
            );
            if let Some(ref config) = secrets_config {
                if let Ok(env) = secrets::resolve_secrets(config) {
                    container_env.extend(env);
                }
            }
            for path in &env_files {
                let resolved = if env_file_relative {
                    resolve_env_file_path(path, &workbook_dir)
                } else {
                    path.to_string()
                };
                if let Ok(env) = secrets::load_env_file(&resolved) {
                    container_env.extend(env);
                }
            }
            let mut vars = workbook.frontmatter.vars.clone().unwrap_or_default();
            vars.extend(cli_vars);
            container_env.extend(vars);
            let _ = artifacts::Artifacts::init(&mut container_env);

            let mut extra_args: Vec<String> = Vec::new();
            if quiet {
                extra_args.push("--quiet".to_string());
            }
            if no_setup {
                extra_args.push("--no-setup".to_string());
            }

            let start = Instant::now();
            let exit_code =
                match sandbox::run_in_sandbox(&tag, file, &container_env, &extra_args, false) {
                    Ok(code) => code,
                    Err(e) => {
                        return output::RunSummary {
                            source_file: file.to_string(),
                            run_id: run_id.clone(),
                            total_blocks: block_count,
                            passed: 0,
                            failed: 1,
                            total_duration: std::time::Duration::ZERO,
                            results: vec![executor::BlockResult {
                                block_index: 0,
                                language: "sandbox".to_string(),
                                stdout: String::new(),
                                stderr: format!("sandbox run: {}", e),
                                exit_code: 1,
                                duration: std::time::Duration::ZERO,
                                error_type: Some("sandbox_failed".to_string()),
                                stdout_partial: false,
                                stderr_partial: false,
                            }],
                        };
                    }
                };

            return output::RunSummary {
                source_file: file.to_string(),
                run_id: run_id.clone(),
                total_blocks: block_count,
                passed: if exit_code == 0 { block_count } else { 0 },
                failed: if exit_code == 0 { 0 } else { 1 },
                total_duration: start.elapsed(),
                results: vec![executor::BlockResult {
                    block_index: 0,
                    language: "sandbox".to_string(),
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code,
                    duration: start.elapsed(),
                    error_type: Some("sandbox_failed".to_string()),
                    stdout_partial: false,
                    stderr_partial: false,
                }],
            };
        }
    }

    let mut ctx = executor::ExecutionContext::from_frontmatter(&workbook.frontmatter, file);
    if let Some(ref d) = dir {
        ctx.working_dir = d.clone();
    }
    ctx.quiet = quiet;

    let secrets_config = build_secrets_config(
        &workbook.frontmatter.secrets,
        secrets_override,
        project,
        secrets_cmd,
    );

    if let Some(ref config) = secrets_config {
        if let Ok(env) = secrets::resolve_secrets(config) {
            ctx.env.extend(env);
        }
    }

    // Load --env-file files (later files override earlier)
    for path in &env_files {
        let resolved = if env_file_relative {
            resolve_env_file_path(path, &ctx.working_dir)
        } else {
            path.to_string()
        };
        match secrets::load_env_file(&resolved) {
            Ok(env) => ctx.env.extend(env),
            Err(e) => {
                return output::RunSummary {
                    source_file: file.to_string(),
                    run_id: run_id.clone(),
                    total_blocks: block_count,
                    passed: 0,
                    failed: 1,
                    total_duration: std::time::Duration::ZERO,
                    results: vec![executor::BlockResult {
                        block_index: 0,
                        language: "env-file".to_string(),
                        stdout: String::new(),
                        stderr: e.to_string(),
                        exit_code: 1,
                        duration: std::time::Duration::ZERO,
                        error_type: Some("env_file_failed".to_string()),
                        stdout_partial: false,
                        stderr_partial: false,
                    }],
                };
            }
        }
    }

    // Merge vars: frontmatter defaults + CLI overrides
    let mut vars = workbook.frontmatter.vars.clone().unwrap_or_default();
    vars.extend(cli_vars);
    ctx.env.extend(vars.clone());
    ctx.vars = vars;

    // Resolved typed parameters (passed by `wb test`); highest precedence.
    ctx.env.extend(extra_env);

    // Build redact values from keys
    let mut redact_keys = workbook.frontmatter.redact.clone().unwrap_or_default();
    redact_keys.extend(cli_redact);
    ctx.redact_values = redact_keys
        .iter()
        .filter_map(|k| ctx.env.get(k))
        .filter(|v| !v.is_empty())
        .cloned()
        .collect();

    // Run setup commands
    if !no_setup {
        if let Some(ref setup) = workbook.frontmatter.setup {
            if let Err(e) = run_setup(setup, &ctx.working_dir) {
                return output::RunSummary {
                    source_file: file.to_string(),
                    run_id: run_id.clone(),
                    total_blocks: block_count,
                    passed: 0,
                    failed: 1,
                    total_duration: std::time::Duration::ZERO,
                    results: vec![executor::BlockResult {
                        block_index: 0,
                        language: "setup".to_string(),
                        stdout: String::new(),
                        stderr: e,
                        exit_code: 1,
                        duration: std::time::Duration::ZERO,
                        error_type: Some("setup_failed".to_string()),
                        stdout_partial: false,
                        stderr_partial: false,
                    }],
                };
            }
        }
    }

    // Artifacts: create the dir and inject WB_ARTIFACTS_DIR into every cell's
    // env so bash/python/browser cells can drop files there. After each cell
    // completes, `artifacts.sync()` picks up new files and uploads them when
    // WB_ARTIFACTS_UPLOAD_URL + WB_RECORDING_UPLOAD_SECRET are set.
    let mut artifacts = artifacts::Artifacts::init(&mut ctx.env);
    let mut run_outputs = step_outputs::RawOutputsByStep::new();
    let outputs_path = step_outputs::init_outputs_path(&mut ctx.env, artifacts.dir(), &run_outputs);

    let start = Instant::now();
    let mut results = Vec::new();
    let mut block_idx = 0;
    let steps = workbook.build_steps();
    let resolved_step_policies = step_ir::resolve_step_policies(&steps, &workbook.frontmatter);
    let fm_default_timeout = workbook
        .frontmatter
        .default_block_timeout_secs()
        .map(Duration::from_secs);
    let effective_default_timeout = fm_default_timeout.or(cli_default_block_timeout);
    let effective_default_source = if fm_default_timeout.is_some() {
        Some(DefaultTimeoutSource::FrontmatterDefault)
    } else if cli_default_block_timeout.is_some() {
        Some(DefaultTimeoutSource::Cli)
    } else {
        None
    };
    let mut session = executor::Session::new(ctx);

    for section in &workbook.sections {
        match section {
            parser::Section::Code(block) => {
                if block.skip_execution
                    || conditional_skip_decision(
                        block.when.as_deref(),
                        block.skip_if.as_deref(),
                        &parser::resolved_env(session.env()),
                    )
                    .is_some()
                {
                    block_idx += 1;
                    continue;
                }
                let policy = step_policy_for(&resolved_step_policies, block_idx);
                let step_id = steps.get(block_idx).map(|s| s.id.as_str());
                let mut result = execute_block_with_policy(
                    &mut session,
                    block,
                    block_idx,
                    policy,
                    effective_default_timeout,
                    effective_default_source,
                    quiet,
                );
                let current_outputs =
                    capture_outputs_for_result(&mut result, policy.continue_on_error);
                if !current_outputs.is_empty() {
                    let key = step_key(step_id, block_idx);
                    step_outputs::merge_step_outputs(&mut run_outputs, &key, &current_outputs);
                    if let Err(e) = step_outputs::write_outputs_file(&outputs_path, &run_outputs) {
                        log_warn!("warning: outputs file: {}", e);
                    }
                }
                // Folder mode has no callback, so artifact records are
                // discarded — no step.artifact_saved is emitted here. The
                // manifest is still recorded so `wb artifacts`/`wb runs` work.
                let synced = artifacts.sync();
                artifacts.record(step_id, &synced);
                results.push(result);
                block_idx += 1;
            }
            parser::Section::Browser(spec) => {
                if spec.skip_execution
                    || conditional_skip_decision(
                        spec.when.as_deref(),
                        spec.skip_if.as_deref(),
                        &parser::resolved_env(session.env()),
                    )
                    .is_some()
                {
                    block_idx += 1;
                    continue;
                }
                let step_id = steps.get(block_idx).map(|s| s.id.as_str());
                let ctx = sidecar::SliceCallbackContext {
                    cb: None,
                    workbook: file,
                    checkpoint_id: None,
                    block_index: block_idx,
                    heading: None,
                    line_number: spec.line_number,
                    completed: block_idx + 1,
                    total: block_count,
                    // Folder mode doesn't emit callbacks, so the chain is
                    // never serialized. An empty slice keeps the type happy.
                    include_chain: &[],
                    step_id,
                    workflow: None,
                };
                let prepared_spec = match prepare_browser_spec(spec, artifacts.dir()) {
                    Ok(prepared) => prepared,
                    Err(e) => {
                        results.push(executor::BlockResult {
                            block_index: block_idx,
                            language: "browser".to_string(),
                            stdout: String::new(),
                            stderr: e,
                            exit_code: 1,
                            duration: std::time::Duration::ZERO,
                            error_type: Some("browser_verb_failed".to_string()),
                            stdout_partial: false,
                            stderr_partial: false,
                        });
                        block_idx += 1;
                        continue;
                    }
                };
                let (mut result, pause) =
                    session.execute_browser_slice(&prepared_spec, block_idx, &ctx, None);
                let current_outputs = capture_outputs_for_result(&mut result, false);
                if !current_outputs.is_empty() {
                    let key = step_key(step_id, block_idx);
                    step_outputs::merge_step_outputs(&mut run_outputs, &key, &current_outputs);
                    if let Err(e) = step_outputs::write_outputs_file(&outputs_path, &run_outputs) {
                        log_warn!("warning: outputs file: {}", e);
                    }
                }
                // Folder mode: no callback, so we don't emit
                // step.artifact_saved for anything this slice produced. Record
                // the manifest anyway for `wb artifacts`/`wb runs`.
                let synced = artifacts.sync();
                artifacts.record(step_id, &synced);
                if pause.is_some() {
                    // Folder mode: no --checkpoint, no way to resume. Fail loudly
                    // rather than leak a half-run browser slice.
                    results.push(executor::BlockResult {
                        block_index: block_idx,
                        language: "browser".to_string(),
                        stdout: result.stdout,
                        stderr: format!(
                            "browser slice paused but folder runs do not support resume (L{}); use --checkpoint",
                            spec.line_number
                        ),
                        exit_code: 1,
                        duration: result.duration,
                        error_type: Some("pause_without_checkpoint".to_string()),
                        stdout_partial: false,
                        stderr_partial: false,
                    });
                    break;
                }
                results.push(result);
                block_idx += 1;
            }
            parser::Section::Wait(spec) => {
                results.push(executor::BlockResult {
                    block_index: block_idx,
                    language: "wait".to_string(),
                    stdout: String::new(),
                    stderr: format!(
                        "wait blocks require --checkpoint (folder runs do not support pause); L{}",
                        spec.line_number
                    ),
                    exit_code: 1,
                    duration: std::time::Duration::ZERO,
                    error_type: Some("wait_without_checkpoint".to_string()),
                    stdout_partial: false,
                    stderr_partial: false,
                });
                break;
            }
            parser::Section::Text(_) => {}
            // Assertions are evaluated by `wb test` (which re-walks the
            // sections against these results); a plain collect run ignores them.
            parser::Section::Expect(_) => {}
            parser::Section::Include(_) => {
                unreachable!(
                    "Section::Include must be resolved by parser::resolve_includes before execution"
                )
            }
            parser::Section::IncludeEnter(_) | parser::Section::IncludeExit(_) => {}
        }
    }

    let total_duration = start.elapsed();
    let passed = results.iter().filter(|r| r.success()).count();
    let failed = results.iter().filter(|r| !r.success()).count();

    output::RunSummary {
        source_file: file.to_string(),
        run_id: run_id.clone(),
        total_blocks: block_count,
        passed,
        failed,
        total_duration,
        results,
    }
}

#[allow(clippy::too_many_arguments)]
/// Bundle of CLI-resolved options for `run_single`. Previously 19 positional
/// params; a struct lets new knobs (params, fence-attr config) land without
/// touching every call site.
struct RunConfig {
    file: String,
    output_path: Option<String>,
    output_format: Option<OutputFormat>,
    stdout_output: bool,
    secrets_override: Option<String>,
    project: Option<String>,
    secrets_cmd: Option<String>,
    dir: Option<String>,
    quiet: bool,
    bail: bool,
    no_setup: bool,
    checkpoint_id: Option<String>,
    callback_url: Option<String>,
    callback_secret: Option<String>,
    callback_key: Option<String>,
    cli_vars: std::collections::HashMap<String, String>,
    cli_redact: Vec<String>,
    env_files: Vec<String>,
    env_file_relative: bool,
    /// Subset selection by step id. `only` runs a single step; `from`/`until`
    /// bound an inclusive range. clap rejects `only` combined with the others
    /// at parse time. All step ids are resolved against the loaded workbook
    /// in `run_single`; an unknown id is a usage error.
    selection: SelectionArgs,
    /// CLI-supplied default block timeout. Sits below frontmatter
    /// `timeouts._default` in precedence. `None` = no CLI default (and, if
    /// frontmatter doesn't set one either, blocks run unbounded).
    default_block_timeout: Option<Duration>,
    /// F7b: set by `wb resume --rerun-step/--goto-step`. Suppresses browser
    /// slice restore so the target slice runs fresh from its first verb
    /// instead of resuming the paused slice at `verb_index + 1`.
    browser_restart: bool,
    /// F7b: executable block indices an operator `goto_step` jumped over. Each
    /// emits `step.skipped` (kind "goto") during the replay prefix instead of
    /// being replayed/executed. Empty for normal runs and resumes.
    skipped_by_goto: std::collections::HashSet<usize>,
    /// Raw `--param KEY=VALUE` inputs, resolved against the workbook's declared
    /// `params:` inside `run_single`.
    param_inputs: Vec<String>,
    /// `--param-file` path (a YAML mapping of name → value).
    param_file: Option<String>,
    /// `--profile` name selecting a block under `profiles:`.
    profile: Option<String>,
    /// `--dry-run`: print the resolved execution plan and exit without running
    /// any block (no sandbox, no secrets, no setup).
    dry_run: bool,
    /// Resolved cache id (`Some` only when `--cache <id>` is set and `--no-cache`
    /// is not). Enables the source-hash execution cache (#18).
    cache_id: Option<String>,
    /// `--repair <url>`: consult this endpoint on a block failure (#42).
    repair_url: Option<String>,
    /// Max repair reruns per failed block.
    repair_max: u32,
    /// `--events <file>`: append each callback event as a JSONL line (#44).
    events_path: Option<String>,
    /// `--sandbox`: run in a `requires:`-style Docker container even when the
    /// workbook declares no `requires:` (OS isolation for untrusted code, #37).
    sandbox: bool,
    /// `--sandbox-no-network`: launch the sandbox container with `--network
    /// none` (an enforceable network allowlist).
    sandbox_no_network: bool,
}

/// CLI-side selection inputs before resolution. Names mirror the flag
/// vocabulary so `clap` field-mapping stays trivial; resolution into a
/// concrete index range happens after the workbook's step list is built.
#[derive(Debug, Default, Clone)]
struct SelectionArgs {
    only: Option<String>,
    from: Option<String>,
    until: Option<String>,
    /// `--tag <class>` (repeatable): run only blocks carrying one of these
    /// fence `.class` tags. Composes with `--from`/`--until` (intersection).
    tag: Vec<String>,
    /// `--changed`: run only blocks new/edited vs the `changed_base` git ref.
    changed: bool,
    /// Git ref `--changed` diffs against (default `HEAD`).
    changed_base: String,
}

impl SelectionArgs {
    fn is_empty(&self) -> bool {
        self.only.is_none()
            && self.from.is_none()
            && self.until.is_none()
            && self.tag.is_empty()
            && !self.changed
    }
}

/// Resolve `--tag` to the set of block indices whose step carries a matching
/// fence `.class`. Returns `None` when no tags were given (no tag filter), or an
/// error if a given tag matches no block (typo guard, mirrors unknown step id).
fn resolve_tag_set(
    sel: &SelectionArgs,
    steps: &[step_ir::Step],
) -> Result<Option<std::collections::BTreeSet<usize>>, String> {
    if sel.tag.is_empty() {
        return Ok(None);
    }
    let mut set = std::collections::BTreeSet::new();
    for tag in &sel.tag {
        let mut matched = false;
        for (idx, step) in steps.iter().enumerate() {
            if step.attrs.classes.iter().any(|c| c == tag) {
                set.insert(idx);
                matched = true;
            }
        }
        if !matched {
            return Err(format!(
                "--tag '{tag}' matches no block (no fence has .{tag})"
            ));
        }
    }
    Ok(Some(set))
}

/// Whether a block index is selected: it must be in the `--from`/`--until`
/// range AND in every active restriction set (`--tag`, `--changed`). An empty
/// `restricts` slice means range-only.
fn block_selected(
    range: &std::ops::Range<usize>,
    restricts: &[std::collections::BTreeSet<usize>],
    idx: usize,
) -> bool {
    range.contains(&idx) && restricts.iter().all(|s| s.contains(&idx))
}

/// Build the active restriction sets (`--tag`, `--changed`) for a run. Each
/// returned set further narrows the `--from`/`--until` range via intersection.
fn resolve_restrictions(
    sel: &SelectionArgs,
    steps: &[step_ir::Step],
    file: &str,
) -> Result<Vec<std::collections::BTreeSet<usize>>, String> {
    let mut restricts = Vec::new();
    if let Some(tags) = resolve_tag_set(sel, steps)? {
        restricts.push(tags);
    }
    if sel.changed {
        let base = if sel.changed_base.is_empty() {
            "HEAD"
        } else {
            sel.changed_base.as_str()
        };
        restricts.push(resolve_changed_set(steps, file, base)?);
    }
    Ok(restricts)
}

/// `--changed`: the set of block indices whose `(language, body)` is NOT present
/// in the same file at the git ref `base` — i.e. blocks that are new or edited.
/// Matching by content (not position) makes it robust to inserting/reordering
/// blocks. An untracked file (or unreadable ref) means "everything changed".
fn resolve_changed_set(
    steps: &[step_ir::Step],
    file: &str,
    base: &str,
) -> Result<std::collections::BTreeSet<usize>, String> {
    let all: std::collections::BTreeSet<usize> = (0..steps.len()).collect();

    // Repo-relative path; empty output ⇒ the file isn't tracked.
    let rel = std::process::Command::new("git")
        .args(["ls-files", "--full-name", "--", file])
        .output()
        .map_err(|e| format!("--changed: git not available: {e}"))?;
    let relpath = String::from_utf8_lossy(&rel.stdout).trim().to_string();
    if relpath.is_empty() {
        eprintln!("wb: --changed: {file} is untracked — treating all blocks as changed");
        return Ok(all);
    }

    let show = std::process::Command::new("git")
        .arg("show")
        .arg(format!("{base}:{relpath}"))
        .output()
        .map_err(|e| format!("--changed: git show failed: {e}"))?;
    if !show.status.success() {
        eprintln!("wb: --changed: no '{relpath}' at {base} — treating all blocks as changed");
        return Ok(all);
    }
    let base_content = String::from_utf8_lossy(&show.stdout);
    let base_steps = parser::parse(&base_content).build_steps();
    let base_bodies: std::collections::HashSet<(String, String)> = base_steps
        .iter()
        .map(|s| (s.language.clone(), s.body.clone()))
        .collect();

    Ok(steps
        .iter()
        .enumerate()
        .filter(|(_, s)| !base_bodies.contains(&(s.language.clone(), s.body.clone())))
        .map(|(i, _)| i)
        .collect())
}

/// If the workbook declares `requires:` and we're not already running inside
/// a `wb` container, build the sandbox image and re-exec `wb` inside Docker.
/// Exits the process directly on re-entry — returns normally only when no
/// sandbox is needed (caller continues with in-process execution).
/// A default `requires:` config synthesized for `--sandbox` when the workbook
/// declares none — a plain container for the workbook's runtime (#37).
fn default_sandbox_requires(workbook: &parser::Workbook) -> parser::RequiresConfig {
    let runtime = workbook
        .frontmatter
        .runtime
        .clone()
        .unwrap_or_else(|| "python".to_string());
    let sandbox = match runtime.as_str() {
        "node" | "javascript" | "js" => "node",
        _ => "python",
    };
    parser::RequiresConfig {
        sandbox: sandbox.to_string(),
        apt: Vec::new(),
        pip: Vec::new(),
        node: Vec::new(),
        dockerfile: None,
    }
}

fn maybe_reenter_sandbox(workbook: &parser::Workbook, cfg: &RunConfig) {
    if std::env::var("WB_SANDBOX_INNER").ok().as_deref() == Some("1") {
        return;
    }
    // Declared `requires:`, or a synthesized container when `--sandbox` forces
    // isolation for a workbook that declares none.
    let synthesized;
    let requires = match workbook.frontmatter.requires.as_ref() {
        Some(r) => r,
        None if cfg.sandbox => {
            synthesized = default_sandbox_requires(workbook);
            &synthesized
        }
        None => return,
    };

    let file = cfg.file.as_str();
    let workbook_dir = std::path::Path::new(file)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    let tag = match sandbox::build_image(requires, &workbook_dir) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: sandbox: {}", e);
            std::process::exit(exit_codes::EXIT_SANDBOX_UNAVAILABLE);
        }
    };

    // Build env for container: resolve secrets + env-files + cli vars
    let mut container_env: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    if let Some(ref fm_env) = workbook.frontmatter.env {
        container_env.extend(fm_env.clone());
    }

    let secrets_config = build_secrets_config(
        &workbook.frontmatter.secrets,
        cfg.secrets_override.clone(),
        cfg.project.clone(),
        cfg.secrets_cmd.clone(),
    );
    if let Some(ref config) = secrets_config {
        if let Ok(env) = secrets::resolve_secrets(config) {
            container_env.extend(env);
        }
    }
    for path in &cfg.env_files {
        let resolved = if cfg.env_file_relative {
            resolve_env_file_path(path, &workbook_dir)
        } else {
            path.to_string()
        };
        if let Ok(env) = secrets::load_env_file(&resolved) {
            container_env.extend(env);
        }
    }
    let mut vars = workbook.frontmatter.vars.clone().unwrap_or_default();
    vars.extend(cfg.cli_vars.clone());
    container_env.extend(vars);
    let _ = artifacts::Artifacts::init(&mut container_env);

    // Forward CLI flags as extra args
    let mut extra_args: Vec<String> = Vec::new();
    if cfg.bail {
        extra_args.push("--bail".to_string());
    }
    if cfg.quiet {
        extra_args.push("--quiet".to_string());
    }
    if cfg.no_setup {
        extra_args.push("--no-setup".to_string());
    }
    if let Some(ref id) = cfg.checkpoint_id {
        extra_args.push("--checkpoint".to_string());
        extra_args.push(id.clone());
    }
    if let Some(ref url) = cfg.callback_url {
        extra_args.push("--callback".to_string());
        extra_args.push(url.clone());
    }
    if let Some(ref secret) = cfg.callback_secret {
        extra_args.push("--callback-secret".to_string());
        extra_args.push(secret.clone());
    }
    if let Some(ref key) = cfg.callback_key {
        extra_args.push("--callback-key".to_string());
        extra_args.push(key.clone());
    }
    if let Some(ref fmt_path) = cfg.output_path {
        extra_args.push("-o".to_string());
        extra_args.push(fmt_path.clone());
    }
    if let Some(ref fmt) = cfg.output_format {
        match fmt {
            OutputFormat::Json => extra_args.push("--json".to_string()),
            OutputFormat::Yaml => extra_args.push("--yaml".to_string()),
            OutputFormat::Markdown => extra_args.push("--md".to_string()),
        }
    }

    let exit_code = match sandbox::run_in_sandbox(
        &tag,
        file,
        &container_env,
        &extra_args,
        cfg.sandbox_no_network,
    ) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: sandbox run: {}", e);
            1
        }
    };
    std::process::exit(exit_code);
}

/// Build the `ExecutionContext` used by the run. Applies, in order:
/// frontmatter defaults, CLI working-dir override, resolved secrets, env
/// files, merged vars (frontmatter defaults + CLI overrides), and redact
/// values. Also runs `setup:` commands unless `--no-setup` was passed.
/// Exits with code 1 on any unrecoverable failure (secrets provider error,
/// missing env file, failing setup) — matches the pre-refactor behavior.
fn build_execution_context(
    workbook: &parser::Workbook,
    cfg: &RunConfig,
    resolved_params: &params::ResolvedParams,
) -> executor::ExecutionContext {
    let mut ctx = executor::ExecutionContext::from_frontmatter(&workbook.frontmatter, &cfg.file);

    if let Some(ref d) = cfg.dir {
        ctx.working_dir = d.clone();
    }

    ctx.quiet = cfg.quiet;

    let secrets_config = build_secrets_config(
        &workbook.frontmatter.secrets,
        cfg.secrets_override.clone(),
        cfg.project.clone(),
        cfg.secrets_cmd.clone(),
    );

    if let Some(ref config) = secrets_config {
        match secrets::resolve_secrets(config) {
            Ok(env) => ctx.env.extend(env),
            Err(e) => {
                eprintln!("error: secrets: {}", e);
                std::process::exit(1);
            }
        }
    }

    // Load --env-file files (later files override earlier)
    for path in &cfg.env_files {
        let resolved = if cfg.env_file_relative {
            resolve_env_file_path(path, &ctx.working_dir)
        } else {
            path.to_string()
        };
        match secrets::load_env_file(&resolved) {
            Ok(env) => ctx.env.extend(env),
            Err(e) => {
                eprintln!("error: env-file: {}", e);
                std::process::exit(1);
            }
        }
    }

    // Merge vars: frontmatter defaults + CLI overrides
    let mut vars = workbook.frontmatter.vars.clone().unwrap_or_default();
    vars.extend(cfg.cli_vars.clone());
    ctx.env.extend(vars.clone());
    ctx.vars = vars;

    // Inject resolved typed parameters under their bare names. Highest
    // precedence over env/secrets/vars so an explicit --param always wins, and
    // visible to {when=}/{skip_if=} since they read the session env.
    ctx.env.extend(
        resolved_params
            .values
            .iter()
            .map(|(k, v)| (k.clone(), v.clone())),
    );

    // Build redact values from keys
    let mut redact_keys = workbook.frontmatter.redact.clone().unwrap_or_default();
    redact_keys.extend(cfg.cli_redact.clone());
    ctx.redact_values = redact_keys
        .iter()
        .filter_map(|k| ctx.env.get(k))
        .filter(|v| !v.is_empty())
        .cloned()
        .collect();
    // Secret-param values are masked even though they aren't named in `redact`.
    ctx.redact_values.extend(
        resolved_params
            .secret_values
            .iter()
            .filter(|v| !v.is_empty())
            .cloned(),
    );

    // Run setup commands
    if !cfg.no_setup {
        if let Some(ref setup) = workbook.frontmatter.setup {
            if let Err(e) = run_setup(setup, &ctx.working_dir) {
                eprintln!("error: {}", e);
                std::process::exit(1);
            }
        }
    }

    ctx
}

/// `--dry-run`: print the resolved execution plan without running anything.
/// Resolves params, selection, conditionals, and per-step policy, then prints
/// for each executable block whether it would run (and the resolved command) or
/// be skipped (and why). Does not resolve secrets or run setup — conditionals
/// are evaluated against frontmatter env + vars + params only.
fn dry_run_preview(
    workbook: &parser::Workbook,
    steps: &[step_ir::Step],
    resolved_step_policies: &[step_ir::ResolvedStepPolicy],
    resolved_params: &params::ResolvedParams,
    cfg: &RunConfig,
    block_count: usize,
) {
    let selection_range = match resolve_selection(&cfg.selection, steps, block_count) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(exit_codes::EXIT_USAGE);
        }
    };
    let selection_restricts = match resolve_restrictions(&cfg.selection, steps, &cfg.file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(exit_codes::EXIT_USAGE);
        }
    };

    // Preview env: frontmatter env + vars + CLI vars + resolved params. No
    // secrets, no env-files, no setup — a dry run has no side effects.
    let mut env: std::collections::HashMap<String, String> =
        workbook.frontmatter.env.clone().unwrap_or_default();
    if let Some(ref v) = workbook.frontmatter.vars {
        env.extend(v.clone());
    }
    env.extend(cfg.cli_vars.clone());
    env.extend(
        resolved_params
            .values
            .iter()
            .map(|(k, v)| (k.clone(), v.clone())),
    );

    let title = workbook.frontmatter.title.as_deref().unwrap_or(&cfg.file);
    println!("{}", output::style_bold(&format!("dry run: {title}")));
    if !resolved_params.values.is_empty() {
        let rendered: Vec<String> = resolved_params
            .values
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        println!("params: {}", rendered.join(" "));
    }
    println!();

    let mut block_idx = 0usize;
    let mut will_run = 0usize;
    let mut will_skip = 0usize;
    let fm = &workbook.frontmatter;
    for section in &workbook.sections {
        let (lang, line, when, skip_if, no_run, detail) = match section {
            parser::Section::Code(b) => (
                b.language.clone(),
                b.line_number,
                b.when.as_deref(),
                b.skip_if.as_deref(),
                b.skip_execution,
                {
                    let l = b.language.to_lowercase();
                    match &fm.exec {
                        Some(parser::ExecConfig::Global(prefix)) => {
                            format!("{} {}", prefix, default_program(&l))
                        }
                        Some(parser::ExecConfig::PerLanguage(map)) => map
                            .get(normalize_block_language(&l))
                            .cloned()
                            .unwrap_or_else(|| default_program(&l).to_string()),
                        None => default_program(&l).to_string(),
                    }
                },
            ),
            parser::Section::Browser(spec) => (
                "browser".to_string(),
                spec.line_number,
                spec.when.as_deref(),
                spec.skip_if.as_deref(),
                spec.skip_execution,
                format!("browser slice ({} verbs)", spec.verbs.len()),
            ),
            _ => continue,
        };

        let step_id = steps
            .get(block_idx)
            .map(|s| s.id.as_str().to_string())
            .unwrap_or_default();
        let policy = step_policy_for(resolved_step_policies, block_idx);

        // Skip precedence mirrors the run loop: selection > no-run > conditional.
        let status = if !block_selected(&selection_range, &selection_restricts, block_idx) {
            "skip (selection)".to_string()
        } else if no_run {
            "skip (no-run)".to_string()
        } else if let Some(d) = conditional_skip_decision(when, skip_if, &env) {
            format!("skip ({})", d.reason)
        } else {
            "run".to_string()
        };

        let mut policy_bits = Vec::new();
        if let Some(t) = policy.timeout_secs {
            policy_bits.push(format!("timeout={t}s"));
        }
        if policy.retries > 0 {
            policy_bits.push(format!("retries={}", policy.retries));
        }
        if policy.continue_on_error {
            policy_bits.push("continue_on_error".to_string());
        }
        let policy_str = if policy_bits.is_empty() {
            String::new()
        } else {
            format!(" [{}]", policy_bits.join(" "))
        };

        if status == "run" {
            will_run += 1;
            println!(
                "  {} [{}] {} (L{}) {} → {}{}",
                output::style_ok("run "),
                block_idx + 1,
                lang,
                line,
                style_step_id(&step_id),
                detail,
                policy_str
            );
        } else {
            will_skip += 1;
            println!(
                "  {} [{}] {} (L{}) {} — {}",
                output::style_dim("skip"),
                block_idx + 1,
                lang,
                line,
                style_step_id(&step_id),
                status
            );
        }
        block_idx += 1;
    }

    println!(
        "\nplan: {} block(s) — {} would run, {} skipped",
        block_count, will_run, will_skip
    );
}

/// Dim a step id for plan output.
fn style_step_id(id: &str) -> String {
    output::style_dim(&format!("#{id}"))
}

/// State pulled out of a run's checkpoint before the execution loop starts.
/// The lock guard must live for the duration of the run — drop it and any
/// other `wb run`/`wb resume` against the same id becomes free to clobber.
struct CheckpointPrep {
    id: Option<String>,
    replay_until: usize,
    results: Vec<executor::BlockResult>,
    ckpt: Option<checkpoint::Checkpoint>,
    lock_guard: Option<atomic_io::FileLock>,
}

/// Outcome of matching a loaded checkpoint against the current workbook.
enum ResumeResolution {
    /// Resume from `replay` (0-based block index). `notice` is an optional
    /// user-facing line printed before the standard "resuming…" message —
    /// used when the position changed (shifted block, fallback path).
    Replay {
        replay: usize,
        notice: Option<String>,
    },
    /// Don't resume — start fresh. `reason` explains why (logged).
    Fresh(String),
}

/// Decide where to resume from given a loaded checkpoint and the current
/// workbook's step list. Pure function — no side effects, no eprintln —
/// so the caller can log uniformly and tests can assert on the variant.
fn resolve_resume_position(
    c: &checkpoint::Checkpoint,
    steps: &[step_ir::Step],
    block_count: usize,
) -> ResumeResolution {
    if let Some(ref sid) = c.next_step_id {
        // Step-id-first: find the same step in the current workbook by id.
        if let Some(pos) = steps.iter().position(|s| s.id.as_str() == sid) {
            let notice = if pos != c.next_block {
                Some(format!(
                    "wb: step '{}' shifted from block {} to block {} since checkpoint was saved",
                    sid,
                    c.next_block + 1,
                    pos + 1
                ))
            } else {
                None
            };
            return ResumeResolution::Replay {
                replay: pos,
                notice,
            };
        }
        // Step id is gone (block deleted / id renamed). Fall back to the
        // numeric block_idx if it's still in range, with a wb-resume-001
        // warning so the operator knows the checkpoint may be stale.
        if c.next_block <= block_count {
            return ResumeResolution::Replay {
                replay: c.next_block,
                notice: Some(format!(
                    "warning [wb-resume-001]: step '{}' not found in current workbook; \
                     falling back to block {} (block may have been removed or id changed)",
                    sid,
                    c.next_block + 1
                )),
            };
        }
        return ResumeResolution::Fresh(format!(
            "step '{}' missing and saved block index ({}) is out of range for current workbook (length {})",
            sid,
            c.next_block + 1,
            block_count
        ));
    }
    // Legacy (v1) checkpoint without a step id. Require the workbook's
    // block count to match — otherwise the numeric block_idx is unsafe.
    if c.total_blocks != block_count {
        return ResumeResolution::Fresh(format!(
            "workbook block count changed ({} → {}) and checkpoint has no step ids to recover from",
            c.total_blocks, block_count
        ));
    }
    ResumeResolution::Replay {
        replay: c.next_block,
        notice: None,
    }
}

/// Resolve a default checkpoint id from the filename if none was supplied,
/// acquire the session-long advisory file lock (exit `EXIT_CHECKPOINT_BUSY`
/// on conflict), load any resumable checkpoint (or create a fresh one), and
/// apply its signal-bound vars into `ctx`. Exits on lock conflict; all other
/// failure modes fall through to a fresh run with a warning.
///
/// Resume strategy is step-id-first: if the loaded checkpoint carries a
/// `next_step_id`, the current workbook is scanned for that id and resume
/// continues from its current position (which may differ from the saved
/// `next_block` if blocks have been inserted/removed above). If the id is
/// absent, the legacy `block_idx` + `total_blocks` path is taken so v1
/// checkpoints keep working.
fn prepare_checkpoint(
    checkpoint_id: Option<String>,
    file: &str,
    block_count: usize,
    steps: &[step_ir::Step],
    param_hash: Option<&str>,
    ctx: &mut executor::ExecutionContext,
) -> CheckpointPrep {
    let id = checkpoint_id.or_else(|| {
        Path::new(file)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
    });

    // Fresh checkpoint stamped with the current param identity, so a later
    // resume can detect a params change and refuse to reuse stale state.
    let mk_fresh = || {
        let mut c = checkpoint::Checkpoint::new(file, block_count);
        c.param_hash = param_hash.map(str::to_string);
        c
    };

    let lock_guard = match id.as_ref() {
        Some(id) => match atomic_io::try_lock_for(&checkpoint::checkpoint_path(id)) {
            Ok(guard) => Some(guard),
            Err(e) => {
                eprintln!(
                    "error: checkpoint '{}' is in use by another process ({}). \
                     refusing to run concurrently — last-writer-wins would lose state.",
                    id, e
                );
                std::process::exit(exit_codes::EXIT_CHECKPOINT_BUSY);
            }
        },
        None => None,
    };

    let (replay_until, results, ckpt) = if let Some(ref id) = id {
        match checkpoint::load(id) {
            Ok(Some(mut c))
                if c.status != checkpoint::CheckpointStatus::Complete && c.workbook == file =>
            {
                // Params changed since the checkpoint was saved → the resolved
                // env differs, so resuming would mix state from two parameter
                // sets. Start fresh.
                if c.param_hash.as_deref() != param_hash {
                    eprintln!("wb: parameters changed since checkpoint was saved; starting fresh");
                    (0, Vec::new(), Some(mk_fresh()))
                } else {
                    // Step-id-first resume: if the checkpoint carries a step id,
                    // locate that step in the *current* workbook and resume from
                    // its position even if blocks shifted. Falls back to the
                    // saved block_idx (plus total_blocks gate) for legacy v1
                    // checkpoints without step ids.
                    let resolution = resolve_resume_position(&c, steps, block_count);
                    match resolution {
                        ResumeResolution::Fresh(reason) => {
                            eprintln!("wb: {}; starting fresh", reason);
                            (0, Vec::new(), Some(mk_fresh()))
                        }
                        ResumeResolution::Replay { replay, notice } => {
                            if let Some(msg) = notice {
                                eprintln!("{}", msg);
                            }
                            eprintln!(
                            "wb: resuming '{}' — replaying {} completed blocks to rebuild state",
                            id, replay
                        );
                            let prior = c.block_results();
                            c.next_block = replay;
                            c.next_step_id = steps.get(replay).map(|s| s.id.0.clone());
                            c.status = checkpoint::CheckpointStatus::InProgress;
                            (replay, prior, Some(c))
                        }
                    }
                }
            }
            Ok(_) => (0, Vec::new(), Some(mk_fresh())),
            Err(e) => {
                log_warn!("warning: {}", e);
                (0, Vec::new(), Some(mk_fresh()))
            }
        }
    } else {
        (0, Vec::new(), None)
    };

    // Merge signal-bound vars from the checkpoint into ctx. Second-line
    // defense against a pre-validation-era checkpoint or a hand-edited state
    // file sneaking a reserved name past parse-time checks.
    if let Some(ref c) = ckpt {
        if !c.bound_vars.is_empty() {
            let filtered: std::collections::HashMap<String, String> = c
                .bound_vars
                .iter()
                .filter(|(k, _)| {
                    if parser::reserved_bind_name(std::iter::once(k.as_str())).is_some() {
                        eprintln!(
                            "wb: refusing to apply bound_var '{}' — reserved env name. Skipping.",
                            k
                        );
                        false
                    } else {
                        true
                    }
                })
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            ctx.vars.extend(filtered.clone());
            ctx.env.extend(filtered);
        }
    }

    CheckpointPrep {
        id,
        replay_until,
        results,
        ckpt,
        lock_guard,
    }
}

/// Build the callback config, falling back through env-file values when CLI
/// flags weren't provided. Returns `None` when there's no URL to post to.
/// `wb config <sub>` — manage machine-wide defaults in `~/.wb/config.yaml`.
/// Shared `--format` handling for management commands. `text` → false,
/// `json` → true, anything else → a usage error.
fn want_json(format: &str) -> Result<bool, WbExit> {
    match format {
        "text" => Ok(false),
        "json" => Ok(true),
        other => Err(WbExit::Usage(format!(
            "invalid --format '{other}' (expected text|json)"
        ))),
    }
}

/// Pretty-print a JSON value to stdout — the management-command convention
/// (matches `inspect`/`validate`/`doctor`/`pending`).
fn print_json(value: &serde_json::Value) {
    match serde_json::to_string_pretty(value) {
        Ok(s) => println!("{s}"),
        Err(e) => log_error!("wb: json serialize error: {e}"),
    }
}

/// `wb version [--format json]`.
fn cmd_version(fmt: &FormatArg) -> WbExit {
    let json = match want_json(&fmt.format) {
        Ok(j) => j,
        Err(e) => return e,
    };
    let version = env!("CARGO_PKG_VERSION");
    if json {
        print_json(&serde_json::json!({ "version": version }));
    } else {
        println!("wb v{version}");
    }
    WbExit::Success
}

fn cmd_config(args: ConfigArgs) -> WbExit {
    match args.sub {
        ConfigSub::Path { fmt } => {
            let json = match want_json(&fmt.format) {
                Ok(j) => j,
                Err(e) => return e,
            };
            let path = config::config_path();
            if json {
                print_json(&serde_json::json!({ "path": path.display().to_string() }));
            } else {
                println!("{}", path.display());
            }
            WbExit::Success
        }
        ConfigSub::List { fmt } => {
            let json = match want_json(&fmt.format) {
                Ok(j) => j,
                Err(e) => return e,
            };
            let cfg = match config::Config::load() {
                Ok(c) => c,
                Err(e) => return WbExit::Io(e),
            };
            if json {
                let known: Vec<_> = config::KNOWN_KEYS
                    .iter()
                    .map(|(k, desc)| serde_json::json!({ "key": k, "description": desc }))
                    .collect();
                print_json(&serde_json::json!({
                    "path": config::config_path().display().to_string(),
                    "values": cfg.values,
                    "known_keys": known,
                }));
                return WbExit::Success;
            }
            if cfg.values.is_empty() {
                println!("no config set ({})", config::config_path().display());
            } else {
                for (k, v) in &cfg.values {
                    println!("{k} = {v}");
                }
            }
            eprintln!("\nknown keys:");
            for (k, desc) in config::KNOWN_KEYS {
                eprintln!("  {k} — {desc}");
            }
            WbExit::Success
        }
        ConfigSub::Get { key, fmt } => {
            let json = match want_json(&fmt.format) {
                Ok(j) => j,
                Err(e) => return e,
            };
            let cfg = match config::Config::load() {
                Ok(c) => c,
                Err(e) => return WbExit::Io(e),
            };
            match cfg.get(&key) {
                Some(v) => {
                    if json {
                        print_json(&serde_json::json!({ "key": key, "value": v }));
                    } else {
                        println!("{v}");
                    }
                    WbExit::Success
                }
                None => {
                    if json {
                        // Still emit a parseable result, but signal "unset" via exit code.
                        print_json(&serde_json::json!({ "key": key, "value": null }));
                    }
                    WbExit::Usage(format!("config key '{key}' is not set"))
                }
            }
        }
        ConfigSub::Set { key, value, fmt } => {
            let json = match want_json(&fmt.format) {
                Ok(j) => j,
                Err(e) => return e,
            };
            if !config::is_known_key(&key) {
                return WbExit::Usage(format!(
                    "unknown config key '{key}'. Run `wb config list` to see known keys"
                ));
            }
            let mut cfg = match config::Config::load() {
                Ok(c) => c,
                Err(e) => return WbExit::Io(e),
            };
            cfg.values.insert(key.clone(), value.clone());
            if let Err(e) = cfg.save() {
                return WbExit::Io(e);
            }
            if json {
                print_json(&serde_json::json!({ "ok": true, "key": key, "value": value }));
            } else {
                eprintln!("set {key} = {value}");
            }
            WbExit::Success
        }
        ConfigSub::Unset { key, fmt } => {
            let json = match want_json(&fmt.format) {
                Ok(j) => j,
                Err(e) => return e,
            };
            let mut cfg = match config::Config::load() {
                Ok(c) => c,
                Err(e) => return WbExit::Io(e),
            };
            let removed = cfg.values.remove(&key).is_some();
            if removed {
                if let Err(e) = cfg.save() {
                    return WbExit::Io(e);
                }
            }
            if json {
                print_json(&serde_json::json!({ "ok": true, "key": key, "removed": removed }));
            } else if removed {
                eprintln!("unset {key}");
            } else {
                eprintln!("{key} was not set");
            }
            WbExit::Success
        }
    }
}

/// `wb completion <shell>` — write a shell completion script to stdout.
fn cmd_completion(shell: clap_complete::Shell) -> WbExit {
    use clap::CommandFactory;
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "wb", &mut std::io::stdout());
    WbExit::Success
}

/// `wb man` — write a roff man page for the top-level command to stdout.
fn cmd_man() -> WbExit {
    use clap::CommandFactory;
    let man = clap_mangen::Man::new(Cli::command());
    if let Err(e) = man.render(&mut std::io::stdout()) {
        return WbExit::Io(format!("failed to render man page: {e}"));
    }
    WbExit::Success
}

fn resolve_callback_config(
    callback_url: Option<String>,
    callback_secret: Option<String>,
    callback_key: Option<String>,
    events_path: Option<String>,
    ctx: &executor::ExecutionContext,
    run_id: &str,
) -> Option<callback::CallbackConfig> {
    // Precedence: CLI flag > env var > ~/.wb/config.yaml. The config file is the
    // "set my dashboard webhook once" layer; a broken file warns, not aborts.
    let cfg = config::Config::load_lenient();
    let url = callback_url
        .or_else(|| ctx.env.get("WB_CALLBACK_URL").cloned())
        .or_else(|| cfg.get("callback.url").map(str::to_string));
    // Nothing to emit to (no endpoint and no local sink) → no callback config.
    if url.is_none() && events_path.is_none() {
        return None;
    }
    let secret = callback_secret
        .or_else(|| ctx.env.get("WB_CALLBACK_SECRET").cloned())
        .or_else(|| cfg.get("callback.secret").map(str::to_string));
    let stream_key = callback_key
        .or_else(|| ctx.env.get("WB_CALLBACK_KEY").cloned())
        .or_else(|| cfg.get("callback.key").map(str::to_string))
        .unwrap_or_else(|| "wb:events".to_string());
    if let Some(ref u) = url {
        match callback::validate_callback_config(u, secret.as_deref()) {
            Ok(warnings) => {
                for w in warnings {
                    log_warn!("warning: {w}");
                }
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(exit_codes::EXIT_USAGE);
            }
        }
    }
    Some(callback::CallbackConfig {
        url: url.unwrap_or_default(),
        secret,
        stream_key,
        run_id: run_id.to_string(),
        seq: std::sync::atomic::AtomicU64::new(0),
        events_path: events_path.map(std::path::PathBuf::from),
    })
}

/// Write the formatted output file and/or stream the rendered format to
/// stdout, per the user's `-o` / `--json|--yaml|--md` flags. No-op when no
/// output format was requested.
fn write_run_output(
    workbook: &parser::Workbook,
    summary: &output::RunSummary,
    output_format: Option<OutputFormat>,
    output_path: Option<&str>,
    stdout_output: bool,
) {
    let Some(fmt) = output_format else { return };
    let rendered = output::format_output(workbook, summary, fmt);

    if stdout_output {
        println!("{}", rendered);
    }

    if let Some(path) = output_path {
        match std::fs::write(path, &rendered) {
            Ok(_) => eprintln!("  -> {}", path),
            Err(e) => eprintln!("error: write {}: {}", path, e),
        }
    }
}

fn run_single(cfg: RunConfig) {
    // Resolve the trace-correlation id once; flows into CallbackConfig and
    // the final RunSummary so every artifact/event of this run shares a key.
    let run_id = artifacts::resolve_run_id(&std::collections::HashMap::new());

    let content = match std::fs::read_to_string(&cfg.file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}: {}", cfg.file, e);
            std::process::exit(1);
        }
    };

    let workbook = parse_and_resolve(&content, &cfg.file);
    let block_count = workbook.code_block_count();
    // Stable step IDs + per-step policies. `steps[i]` is index-aligned with the
    // run loop's `block_idx` (i.e. matches the legacy `(block_idx + 1)`-based
    // block number used by `frontmatter.block_policy`). Built once here so the
    // ids remain stable across replay/resume of this run.
    let steps = workbook.build_steps();
    let workflow_ctx = workflow::WorkflowContext::from_frontmatter(&workbook.frontmatter);
    let resolved_step_policies = step_ir::resolve_step_policies(&steps, &workbook.frontmatter);

    if block_count == 0 {
        eprintln!(
            "no executable blocks in {}. Known runtimes: bash, sh, zsh, python, node, ruby, perl, r, php, lua, swift, go. \
             Check your fence language tags — `{{no-run}}` and `{{silent}}` are stable as of v0.9.8.",
            cfg.file
        );
        std::process::exit(exit_codes::EXIT_USAGE);
    }

    // Resolve declared typed parameters against the CLI inputs. A bad value,
    // unknown param/profile, or a missing required param is a usage error
    // before any block runs. Done before sandbox re-entry so --dry-run and
    // param errors surface on the host, not inside a container.
    let resolved_params = match params::resolve(
        workbook.frontmatter.params.as_ref(),
        workbook.frontmatter.profiles.as_ref(),
        cfg.profile.as_deref(),
        cfg.param_file.as_deref(),
        &cfg.param_inputs,
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(exit_codes::EXIT_USAGE);
        }
    };
    let param_hash = resolved_params.hash.clone();

    // --dry-run: print the resolved execution plan and exit without running
    // anything (no sandbox, no secrets, no setup, no blocks).
    if cfg.dry_run {
        dry_run_preview(
            &workbook,
            &steps,
            &resolved_step_policies,
            &resolved_params,
            &cfg,
            block_count,
        );
        return;
    }

    maybe_reenter_sandbox(&workbook, &cfg);

    let mut ctx = build_execution_context(&workbook, &cfg, &resolved_params);

    // Sandbox didn't re-enter; ctx is built. Destructure cfg now so the
    // rest of the function (checkpoint, callback, execution loop) uses
    // bare locals instead of `cfg.field` everywhere.
    let RunConfig {
        file,
        output_path,
        output_format,
        stdout_output,
        secrets_override: _,
        project: _,
        secrets_cmd: _,
        dir: _,
        quiet,
        bail,
        no_setup: _,
        checkpoint_id,
        callback_url,
        callback_secret,
        callback_key,
        cli_vars: _,
        cli_redact: _,
        env_files: _,
        env_file_relative: _,
        selection,
        default_block_timeout: cli_default_block_timeout,
        browser_restart,
        skipped_by_goto,
        param_inputs: _,
        param_file: _,
        profile: _,
        dry_run: _,
        cache_id,
        repair_url,
        repair_max,
        events_path,
        sandbox: _,
        sandbox_no_network: _,
    } = cfg;
    let file = file.as_str();
    // Resolve the run-wide default block timeout once. Precedence:
    // frontmatter `timeouts._default` > CLI `--default-block-timeout` > None.
    // The per-block override (fence attr or frontmatter[N]) is applied later
    // inside `execute_block_with_policy` and beats both. `effective_default_source`
    // tracks which knob set the default so the timeout error message can name it.
    let fm_default_timeout = workbook
        .frontmatter
        .default_block_timeout_secs()
        .map(Duration::from_secs);
    let effective_default_timeout = fm_default_timeout.or(cli_default_block_timeout);
    let effective_default_source = if fm_default_timeout.is_some() {
        Some(DefaultTimeoutSource::FrontmatterDefault)
    } else if cli_default_block_timeout.is_some() {
        Some(DefaultTimeoutSource::Cli)
    } else {
        None
    };

    // Resolve `--only` / `--from` / `--until` BEFORE prepare_checkpoint so a
    // selective run can opt out of implicit checkpointing — partial-run state
    // semantics under a checkpoint id aren't defined yet, and an inherited
    // resume would silently skip the user's selection.
    let selection_range = match resolve_selection(&selection, &steps, block_count) {
        Ok(range) => range,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(exit_codes::EXIT_USAGE);
        }
    };
    let selection_restricts = match resolve_restrictions(&selection, &steps, file) {
        Ok(set) => set,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(exit_codes::EXIT_USAGE);
        }
    };
    // "User opted into selection" — independent of whether the resolved
    // range happens to equal 0..block_count (e.g. `--only` of a one-block
    // workbook). That's what controls checkpoint conflict + suppression.
    let selection_active = !selection.is_empty();
    if selection_active && checkpoint_id.is_some() {
        eprintln!(
            "error: --only/--from/--until/--tag cannot be combined with --checkpoint yet. \
             Selective-run checkpoint semantics aren't defined (which 'completed' do we \
             track when most blocks are intentionally skipped?). Drop --checkpoint to run \
             ephemerally, or remove the selection flags to resume normally."
        );
        std::process::exit(exit_codes::EXIT_USAGE);
    }
    let effective_checkpoint_id = if selection_active {
        if !quiet {
            eprintln!("wb: selective run — checkpointing disabled");
        }
        None
    } else {
        checkpoint_id
    };

    let CheckpointPrep {
        id: checkpoint_id,
        replay_until,
        mut results,
        mut ckpt,
        lock_guard: _checkpoint_lock,
    } = prepare_checkpoint(
        effective_checkpoint_id,
        file,
        block_count,
        &steps,
        param_hash.as_deref(),
        &mut ctx,
    );

    if let Some(ref mut c) = ckpt {
        c.workflow = workbook.frontmatter.workflow.clone();
        // Persist the resolved params + identity so a later `wb resume` can
        // re-apply them (resume carries no --param flags) and so a params
        // change is detected on the next run.
        c.param_hash = param_hash.clone();
        c.params = resolved_params.values.clone();
    }
    let mut run_outputs = ckpt.as_ref().map(|c| c.outputs.clone()).unwrap_or_default();

    let cb = resolve_callback_config(
        callback_url,
        callback_secret,
        callback_key,
        events_path,
        &ctx,
        &run_id,
    );
    // Resume: continue the X-WB-Sequence counter from where the pre-pause
    // process left off, so a run's HTTP callbacks stay totally orderable by
    // sequence even across pause/resume (the counter is otherwise per-process).
    if let (Some(cb), Some(c)) = (&cb, &ckpt) {
        if c.callback_seq > 0 {
            cb.set_seq(c.callback_seq);
        }
    }

    // Artifacts: same semantics as the non-checkpoint path — create/read the
    // dir, inject WB_ARTIFACTS_DIR, upload new files after each cell.
    let mut artifacts = artifacts::Artifacts::init(&mut ctx.env);
    let outputs_path = step_outputs::init_outputs_path(&mut ctx.env, artifacts.dir(), &run_outputs);

    let start = Instant::now();
    let mut block_idx = 0;
    let mut session = executor::Session::new(ctx);

    // Source-hash execution cache (#18): load the store when `--cache <id>` is
    // active. A block whose (language, body, param) key matches a prior success
    // is skipped; new successes are recorded and the store is saved at run end.
    let mut cache_store = cache_id.as_ref().map(|id| cache::CacheStore::load(id));
    let cache_now = chrono::Utc::now().to_rfc3339();
    // Env/secret identity for the cache key (#18) — wb-managed env minus the
    // run-specific WB_* internals. Snapshotted before the loop so WB_OUT_*
    // exports during the run don't bust it (they're WB_*-prefixed anyway).
    let cache_env_hash = cache::env_identity(session.env());

    if !quiet {
        let title = workbook.frontmatter.title.as_deref().unwrap_or(file);
        eprintln!("{}", output::style_bold(title));
    }

    let mut last_heading: Option<String> = None;

    // Include-boundary tracking for step.started / step.finished events.
    // The stack is the source of truth for `include_chain` enrichment on every
    // block/pause callback. `frame_starts` parallels the stack so we can
    // compute duration_ms on Exit. On resume, the stack is rebuilt by
    // replaying IncludeEnter/Exit sentinels; `emitted_resume_chain` ensures
    // we fire step.started once for the restored stack before the first live
    // action, giving the run page a fresh timeline after a pause.
    let mut include_stack: Vec<parser::IncludeFrame> = Vec::new();
    let mut frame_starts: Vec<Instant> = Vec::new();
    let mut emitted_resume_chain = replay_until == 0;

    // Check for stale code hashes before replay
    if replay_until > 0 {
        if let Some(ref c) = ckpt {
            let mut stale_warned = false;
            let mut check_idx = 0;
            for section in &workbook.sections {
                if check_idx >= replay_until {
                    break;
                }
                match section {
                    parser::Section::Code(block) => {
                        if let Some(saved) = c.results.iter().find(|r| r.block_index == check_idx) {
                            if let Some(ref saved_hash) = saved.code_hash {
                                let current_hash = checkpoint::hash_code(&block.code);
                                if *saved_hash != current_hash {
                                    if !stale_warned {
                                        eprintln!(
                                        "{}",
                                        output::style_fail(
                                            "warning: block source changed since last checkpoint:"
                                        )
                                    );
                                        stale_warned = true;
                                    }
                                    eprintln!(
                                        "  block {} [{}] L{}",
                                        check_idx + 1,
                                        block.language,
                                        block.line_number
                                    );
                                }
                            }
                        }
                        check_idx += 1;
                    }
                    parser::Section::Browser(_) => {
                        check_idx += 1;
                    }
                    _ => {}
                }
            }
        }
    }

    let mut replay_cleaned = replay_until == 0;

    // If we were called by `cmd_resume` to resume a paused browser slice, the
    // pending descriptor carries the opaque sidecar state + the resolver's
    // signal payload. Consumed once by the paused block.
    let mut browser_restore: Option<sidecar::RestoreArgs> = None;
    if let Some(ref id) = checkpoint_id {
        if let Ok(Some(desc)) = pending::load(id) {
            let resume_signal = std::env::var("WB_BROWSER_RESUME_SIGNAL")
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
            // pause_for_human with `actions:` → persist the operator's choice
            // to $WB_ARTIFACTS_DIR/pause_result.json so downstream cells can
            // branch on it via a plain file read. Skipped when actions is
            // empty (single default "Resume" button — nothing to record) or
            // no signal came in.
            if !desc.actions.is_empty() {
                write_pause_result(artifacts.dir(), resume_signal.as_ref());
            }
            // F7b: a rerun/goto resume runs the target slice fresh from verb 0,
            // so skip restoring the paused slice's sidecar state.
            if desc.sidecar_state.is_some() && !browser_restart {
                browser_restore = Some(sidecar::RestoreArgs {
                    state: desc.sidecar_state.clone(),
                    signal: resume_signal.clone(),
                });
            }
        }
    }

    for (section_idx, section) in workbook.sections.iter().enumerate() {
        // Once we've advanced past the replay prefix on a resumed run, re-emit
        // step.started for frames rebuilt during replay so the run page sees
        // a fresh timeline. Fires at most once per run; no-op on cold starts
        // (replay_until == 0 → emitted_resume_chain starts true).
        if !emitted_resume_chain && block_idx >= replay_until {
            if let Some(ref cb) = cb {
                for (i, frame) in include_stack.iter().enumerate() {
                    let parent = if i > 0 {
                        Some(include_stack[i - 1].id.as_str())
                    } else {
                        None
                    };
                    cb.step_started(file, checkpoint_id.as_deref(), frame, parent);
                }
            }
            emitted_resume_chain = true;
        }

        if let parser::Section::IncludeEnter(frame) = section {
            let parent_id = include_stack.last().map(|f| f.id.clone());
            include_stack.push(frame.clone());
            frame_starts.push(Instant::now());
            // Suppress callback during replay: the prior run already fired
            // step.started. We push to rebuild the stack silently.
            if block_idx >= replay_until {
                if let Some(ref cb) = cb {
                    cb.step_started(file, checkpoint_id.as_deref(), frame, parent_id.as_deref());
                }
            }
            continue;
        }

        if let parser::Section::IncludeExit(_) = section {
            let popped = include_stack.pop();
            let start = frame_starts.pop();
            let parent_id = include_stack.last().map(|f| f.id.clone());
            if block_idx >= replay_until {
                if let (Some(ref cb), Some(frame), Some(started)) = (&cb, popped, start) {
                    cb.step_finished(
                        file,
                        checkpoint_id.as_deref(),
                        &frame,
                        parent_id.as_deref(),
                        started.elapsed().as_millis() as u64,
                        "ok",
                    );
                }
            }
            continue;
        }

        if let parser::Section::Text(text) = section {
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("## ") {
                    last_heading = Some(trimmed.trim_start_matches('#').trim().to_string());
                }
            }
        }

        if let parser::Section::Wait(spec) = section {
            let wait_step_id = spec.attrs.explicit_id.as_deref();
            let workflow_payload = workflow_ctx
                .as_ref()
                .and_then(|w| w.payload_for_step(wait_step_id));

            // Skip waits already satisfied by a prior resume.
            let already_done = ckpt
                .as_ref()
                .map(|c| c.waits_completed.contains(&section_idx))
                .unwrap_or(false);
            if already_done {
                if !quiet {
                    let bind_label = spec
                        .bind
                        .as_ref()
                        .map(|b| match b {
                            parser::BindSpec::Single(s) => s.clone(),
                            parser::BindSpec::Multiple(v) => v.join(","),
                        })
                        .unwrap_or_else(|| "-".to_string());
                    eprintln!(
                        "{}",
                        output::style_dim(&format!(
                            "  ↻ wait satisfied (L{}, bind={})",
                            spec.line_number, bind_label
                        ))
                    );
                }
                continue;
            }

            // pause_for_signal diverges via std::process::exit, which skips
            // Drop on `session` — the browser sidecar would be SIGKILLed by
            // the OS instead of getting its graceful-shutdown window. Drop
            // explicitly so Sidecar::Drop fires (shutdown frame + recording
            // flush + browser.close() before exit).
            drop(session);
            // At a Wait section, block_idx points at the next code/browser
            // step (waits don't increment block_idx). That's the resume entry
            // point — persist its stable id alongside the numeric block_idx.
            let next_step_id = steps.get(block_idx).map(|s| s.id.0.clone());
            pause_for_signal(
                spec,
                section_idx,
                &checkpoint_id,
                ckpt.as_mut(),
                file,
                block_idx,
                next_step_id.as_deref(),
                block_count,
                start.elapsed(),
                &results,
                cb.as_ref(),
                &include_stack,
                &frame_starts,
                workflow_payload.as_ref(),
            );
            // Unreachable — pause_for_signal exits.
        }

        if let parser::Section::Code(block) = section {
            let step_id = steps.get(block_idx).map(|s| s.id.as_str());
            let workflow_payload = workflow_ctx
                .as_ref()
                .and_then(|w| w.payload_for_step(step_id));

            // Replay completed blocks to rebuild session state
            if block_idx < replay_until {
                // F7b: an operator goto_step jumped past this step — emit
                // step.skipped (kind "goto") instead of replaying/executing it.
                if skipped_by_goto.contains(&block_idx) {
                    let block_heading = last_heading.take();
                    if !quiet {
                        eprintln!(
                            "{}",
                            output::style_dim(&format!(
                                "  ⊘ skipped [{}] (L{}) — goto_step",
                                block.language, block.line_number
                            ))
                        );
                    }
                    let skip = goto_skip_decision();
                    if let Some(ref cb) = cb {
                        if should_emit_skip_callback(block.silent, workflow_payload.as_ref()) {
                            cb.step_skipped(
                                file,
                                checkpoint_id.as_deref(),
                                block_idx,
                                step_id,
                                &block.language,
                                block_heading.as_deref(),
                                block.line_number,
                                block_idx + 1,
                                block_count,
                                &skip.kind,
                                skip.expression.as_deref(),
                                &skip.reason,
                                &include_stack,
                                workflow_payload.as_ref(),
                            );
                        }
                    }
                    if let (Some(ref mut c), Some(ref ckpt_id)) = (&mut ckpt, &checkpoint_id) {
                        c.add_skip(checkpoint::SavedSkip {
                            block_index: block_idx,
                            step_id: step_id.map(|s| s.to_string()),
                            language: block.language.clone(),
                            line_number: block.line_number,
                            heading: block_heading.clone(),
                            kind: skip.kind,
                            expression: skip.expression,
                            reason: skip.reason,
                            code_hash: Some(checkpoint::hash_code(&block.code)),
                        });
                        c.next_step_id = steps.get(c.next_block).map(|s| s.id.0.clone());
                        if let Err(e) = checkpoint::save(ckpt_id, c) {
                            log_warn!("warning: checkpoint: {}", e);
                        }
                    }
                    block_idx += 1;
                    continue;
                }

                let replayed_skip = ckpt
                    .as_ref()
                    .and_then(|c| c.skipped_step(block_idx))
                    .cloned();
                if replayed_skip.is_some() || block.skip_execution {
                    if !quiet {
                        eprintln!(
                            "{}",
                            output::style_dim(&format!(
                                "  ↻ skipped [{}/{}] {} (L{})",
                                block_idx + 1,
                                block_count,
                                block.language,
                                block.line_number
                            ))
                        );
                    }
                    last_heading = None;
                    block_idx += 1;
                    continue;
                }

                let replay_line = format!(
                    "  ↻ replaying [{}/{}] {} (L{})",
                    block_idx + 1,
                    block_count,
                    block.language,
                    block.line_number
                );
                eprintln!("{}", output::style_dim(&replay_line));

                // Execute with quiet=true and WB_REPLAY=1
                session.set_quiet(true);
                session.set_env("WB_REPLAY".to_string(), "1".to_string());
                let mut replay_result = session.execute_block(block, block_idx);
                session.set_quiet(quiet);
                let replay_outputs = capture_outputs_for_result(&mut replay_result, true);
                if replay_result.success() && !replay_outputs.is_empty() {
                    let key = step_key(step_id, block_idx);
                    step_outputs::merge_step_outputs(&mut run_outputs, &key, &replay_outputs);
                    if let Err(e) = step_outputs::write_outputs_file(&outputs_path, &run_outputs) {
                        log_warn!("warning: outputs file: {}", e);
                    }
                    if let (Some(ref mut c), Some(ref ckpt_id)) = (&mut ckpt, &checkpoint_id) {
                        c.add_outputs(&key, &raw_outputs(&replay_outputs));
                        let _ = checkpoint::save(ckpt_id, c);
                    }
                }

                if !replay_result.success() {
                    eprintln!(
                        "{}",
                        output::style_fail(&format!(
                            "  warning: replay block {} failed (exit {})",
                            block_idx + 1,
                            replay_result.exit_code
                        ))
                    );
                }

                last_heading = None;
                block_idx += 1;
                continue;
            }

            // Clean up WB_REPLAY from running sessions after replay completes
            if !replay_cleaned {
                session.remove_env("WB_REPLAY");
                session.unset_env_in_sessions("WB_REPLAY");
                replay_cleaned = true;
            }

            let block_heading = last_heading.take();
            let live_skip = if !block_selected(&selection_range, &selection_restricts, block_idx) {
                Some(selection_skip_decision())
            } else if block.skip_execution {
                Some(no_run_skip_decision())
            } else {
                conditional_skip_decision(
                    block.when.as_deref(),
                    block.skip_if.as_deref(),
                    &parser::resolved_env(session.env()),
                )
            };
            // Cache skip (#18): if nothing else skipped this block and `--cache`
            // has a successful entry for its source+params, skip it.
            let live_skip = live_skip.or_else(|| {
                let store = cache_store.as_ref()?;
                if block.no_cache {
                    return None;
                }
                let key = cache::cache_key(
                    &block.language,
                    &block.code,
                    param_hash.as_deref(),
                    &cache_env_hash,
                    &cache::artifact_inputs_hash(
                        block.attrs.kv.get("reads").map(String::as_str),
                        artifacts.dir(),
                    ),
                );
                store.is_cached_success(&key).then(cache_skip_decision)
            });
            if let Some(skip) = live_skip {
                if !quiet {
                    let label = if skip.kind == "no_run" {
                        format!(
                            "  ⊘ skipped {{no-run}} [{}] (L{})",
                            block.language, block.line_number
                        )
                    } else {
                        format!(
                            "  ⊘ skipped [{}] (L{}) — {}",
                            block.language, block.line_number, skip.reason
                        )
                    };
                    eprintln!("{}", output::style_dim(&label));
                }
                if let Some(ref cb) = cb {
                    if should_emit_skip_callback(block.silent, workflow_payload.as_ref()) {
                        cb.step_skipped(
                            file,
                            checkpoint_id.as_deref(),
                            block_idx,
                            step_id,
                            &block.language,
                            block_heading.as_deref(),
                            block.line_number,
                            block_idx + 1,
                            block_count,
                            &skip.kind,
                            skip.expression.as_deref(),
                            &skip.reason,
                            &include_stack,
                            workflow_payload.as_ref(),
                        );
                    }
                }
                if let (Some(ref mut c), Some(ref ckpt_id)) = (&mut ckpt, &checkpoint_id) {
                    c.add_skip(checkpoint::SavedSkip {
                        block_index: block_idx,
                        step_id: step_id.map(|s| s.to_string()),
                        language: block.language.clone(),
                        line_number: block.line_number,
                        heading: block_heading.clone(),
                        kind: skip.kind,
                        expression: skip.expression,
                        reason: skip.reason,
                        code_hash: Some(checkpoint::hash_code(&block.code)),
                    });
                    c.next_step_id = steps.get(c.next_block).map(|s| s.id.0.clone());
                    if let Err(e) = checkpoint::save(ckpt_id, c) {
                        log_warn!("warning: checkpoint: {}", e);
                    }
                }
                block_idx += 1;
                continue;
            }

            if !quiet {
                let preview = block.code.lines().next();
                output::print_block_header(
                    block_heading.as_deref(),
                    &block.language,
                    block.line_number,
                    preview,
                );
            }

            let policy = step_policy_for(&resolved_step_policies, block_idx);
            let mut result = execute_block_with_policy(
                &mut session,
                block,
                block_idx,
                policy,
                effective_default_timeout,
                effective_default_source,
                quiet,
            );

            // Self-healing (#42): on failure, consult the repair endpoint and
            // apply rerun/skip/abort/patch. Bounded by repair_max to prevent
            // loops. `patch` runs endpoint-supplied code (opt-in; see below).
            let mut repair_skip = false;
            if let Some(ref url) = repair_url {
                let mut budget = repair_max;
                while !result.success() && !policy.continue_on_error {
                    match repair_consult(url, &result, block_idx, &block.language, &session) {
                        RepairAction::Rerun if budget > 0 => {
                            budget -= 1;
                            if !quiet {
                                eprintln!(
                                    "  ↻ repair: re-running block {} ({} left)",
                                    block_idx + 1,
                                    budget
                                );
                            }
                            result = execute_block_with_policy(
                                &mut session,
                                block,
                                block_idx,
                                policy,
                                effective_default_timeout,
                                effective_default_source,
                                quiet,
                            );
                        }
                        RepairAction::Patch(code) if budget > 0 => {
                            budget -= 1;
                            // Endpoint-supplied code. Logged prominently — never
                            // silent — since `--repair` is an explicit opt-in.
                            eprintln!(
                                "  ✚ repair: applying patched command from endpoint for block {} ({} left):\n      {}",
                                block_idx + 1,
                                budget,
                                code.lines().next().unwrap_or("")
                            );
                            let patched = parser::CodeBlock {
                                language: block.language.clone(),
                                code,
                                line_number: block.line_number,
                                skip_execution: false,
                                silent: block.silent,
                                when: None,
                                skip_if: None,
                                no_cache: true,
                                attrs: Default::default(),
                            };
                            result = execute_block_with_policy(
                                &mut session,
                                &patched,
                                block_idx,
                                policy,
                                effective_default_timeout,
                                effective_default_source,
                                quiet,
                            );
                        }
                        RepairAction::Skip => {
                            if !quiet {
                                eprintln!("  ⤼ repair: skipping failed block {}", block_idx + 1);
                            }
                            repair_skip = true;
                            break;
                        }
                        // Abort, unknown, or budget-exhausted: stop repairing.
                        _ => break,
                    }
                }
            }

            // Record this block's outcome in the source-hash cache (#18) so a
            // future `--cache` run can skip it when unchanged.
            if let Some(ref mut store) = cache_store {
                if !block.no_cache {
                    let key = cache::cache_key(
                        &block.language,
                        &block.code,
                        param_hash.as_deref(),
                        &cache_env_hash,
                        &cache::artifact_inputs_hash(
                            block.attrs.kv.get("reads").map(String::as_str),
                            artifacts.dir(),
                        ),
                    );
                    store.record(key, result.success(), result.exit_code, &cache_now);
                }
            }
            let current_outputs = capture_outputs_for_result(&mut result, policy.continue_on_error);
            if !current_outputs.is_empty() {
                let key = step_key(step_id, block_idx);
                step_outputs::merge_step_outputs(&mut run_outputs, &key, &current_outputs);
                // Export outputs into the session env (WB_OUT_<name>) so later
                // cells' `{when=...}` / `{skip_if=...}` can branch on a value
                // this step produced.
                step_outputs::export_to_session(&mut session, &current_outputs);
                if let Err(e) = step_outputs::write_outputs_file(&outputs_path, &run_outputs) {
                    log_warn!("warning: outputs file: {}", e);
                }
                if let (Some(ref mut c), Some(ref ckpt_id)) = (&mut ckpt, &checkpoint_id) {
                    c.add_outputs(&key, &raw_outputs(&current_outputs));
                    let _ = checkpoint::save(ckpt_id, c);
                }
            }
            let callback_outputs = if current_outputs.is_empty() {
                None
            } else {
                Some(step_outputs::callback_outputs(&current_outputs))
            };
            let new_artifacts = artifacts.sync();
            artifacts.record(step_id, &new_artifacts);

            // Emit step.artifact_saved for each new artifact produced by this
            // block, before step.complete — ordering groups artifacts under
            // the step that produced them in the run-page timeline. Silent
            // blocks suppress the events: `{silent}` is a hard off-switch.
            if let Some(ref cb) = cb {
                if !block.silent {
                    for art in &new_artifacts {
                        cb.step_artifact_saved(
                            file,
                            checkpoint_id.as_deref(),
                            block_idx,
                            &block.language,
                            block_heading.as_deref(),
                            block.line_number,
                            block_idx + 1,
                            block_count,
                            &art.filename,
                            &art.path.to_string_lossy(),
                            art.bytes,
                            art.content_type,
                            art.label.as_deref(),
                            art.description.as_deref(),
                            &include_stack,
                            step_id,
                            workflow_payload.as_ref(),
                        );
                    }
                }
            }
            let success = result.success();

            // Per-block progress
            let status_line = format!(
                "{} [{}/{}] {} ({:.1}s)",
                if success { "✓" } else { "✗" },
                block_idx + 1,
                block_count,
                block.language,
                result.duration.as_secs_f64()
            );
            if success {
                eprintln!("{}", output::style_ok(&status_line));
            } else {
                eprintln!("{}", output::style_fail(&status_line));
            }

            // Callback: step complete (fires for every executed block unless `{silent}`)
            if let Some(ref cb) = cb {
                if !block.silent {
                    cb.step_complete(
                        &result,
                        block_idx + 1,
                        block_count,
                        file,
                        checkpoint_id.as_deref(),
                        block_heading.as_deref(),
                        block.line_number,
                        &include_stack,
                        step_id,
                        callback_outputs.as_ref(),
                        workflow_payload.as_ref(),
                    );
                }
            }

            if bail && !success && !policy.continue_on_error && !repair_skip {
                // Callback: checkpoint failed (when checkpointing is active).
                // `{silent}` only gates step.complete/step.failed — a failed
                // silent block still produces a checkpoint.failed event so
                // agent orchestrators know a run needs intervention.
                if let (Some(ref cb), Some(ref ckpt_id)) = (&cb, &checkpoint_id) {
                    cb.checkpoint_failed(
                        &result,
                        block_idx,
                        block_count,
                        file,
                        ckpt_id,
                        block_heading.as_deref(),
                        block.line_number,
                        &include_stack,
                        step_id,
                        workflow_payload.as_ref(),
                    );
                }

                // Bubble the failure up the active include stack: fire
                // step.finished(outcome="failed") deepest-first so consumers
                // see each parent include close with the same terminal state.
                emit_finish_for_stack(
                    cb.as_ref(),
                    file,
                    checkpoint_id.as_deref(),
                    &mut include_stack,
                    &mut frame_starts,
                    "failed",
                );

                // Don't checkpoint the failed block — re-run it on resume
                if let (Some(ref mut c), Some(ref ckpt_id)) = (&mut ckpt, &checkpoint_id) {
                    c.mark_failed();
                    let _ = checkpoint::save(ckpt_id, c);
                }
                results.push(result);
                break;
            }

            if !success && policy.continue_on_error && !quiet {
                eprintln!(
                    "  ⚠ block {} failed but continue_on_error set — moving on",
                    block_idx + 1
                );
            }

            // Checkpoint after each successful block (or any block without bail)
            if let (Some(ref mut c), Some(ref ckpt_id)) = (&mut ckpt, &checkpoint_id) {
                c.add_result(
                    &result,
                    block.line_number,
                    block_heading.as_deref(),
                    &block.code,
                    step_id,
                );
                c.next_step_id = steps.get(c.next_block).map(|s| s.id.0.clone());
                if let Err(e) = checkpoint::save(ckpt_id, c) {
                    log_warn!("warning: checkpoint: {}", e);
                }
            }

            results.push(result);
            block_idx += 1;
        }

        if let parser::Section::Browser(spec) = section {
            let step_id = steps.get(block_idx).map(|s| s.id.as_str());
            let workflow_payload = workflow_ctx
                .as_ref()
                .and_then(|w| w.payload_for_step(step_id));

            // Replay path: browser sidecars rehydrate via persistent Browserbase
            // contexts, so a completed slice doesn't need to re-execute.
            if block_idx < replay_until {
                // F7b: an operator goto_step jumped past this slice — emit
                // step.skipped (kind "goto") rather than treating it as a
                // silently-replayed completed slice.
                if skipped_by_goto.contains(&block_idx) {
                    let block_heading = last_heading.take();
                    if !quiet {
                        eprintln!(
                            "{}",
                            output::style_dim(&format!(
                                "  ⊘ skipped [browser] (L{}) — goto_step",
                                spec.line_number
                            ))
                        );
                    }
                    let skip = goto_skip_decision();
                    if let Some(ref cb) = cb {
                        if should_emit_skip_callback(spec.silent, workflow_payload.as_ref()) {
                            cb.step_skipped(
                                file,
                                checkpoint_id.as_deref(),
                                block_idx,
                                step_id,
                                "browser",
                                block_heading.as_deref(),
                                spec.line_number,
                                block_idx + 1,
                                block_count,
                                &skip.kind,
                                skip.expression.as_deref(),
                                &skip.reason,
                                &include_stack,
                                workflow_payload.as_ref(),
                            );
                        }
                    }
                    if let (Some(ref mut c), Some(ref ckpt_id)) = (&mut ckpt, &checkpoint_id) {
                        c.add_skip(checkpoint::SavedSkip {
                            block_index: block_idx,
                            step_id: step_id.map(|s| s.to_string()),
                            language: "browser".to_string(),
                            line_number: spec.line_number,
                            heading: block_heading.clone(),
                            kind: skip.kind,
                            expression: skip.expression,
                            reason: skip.reason,
                            code_hash: None,
                        });
                        c.next_step_id = steps.get(c.next_block).map(|s| s.id.0.clone());
                        if let Err(e) = checkpoint::save(ckpt_id, c) {
                            log_warn!("warning: checkpoint: {}", e);
                        }
                    }
                    block_idx += 1;
                    continue;
                }

                let replay_line = format!(
                    "  ↻ replaying [{}/{}] browser (L{}) — skipped",
                    block_idx + 1,
                    block_count,
                    spec.line_number
                );
                eprintln!("{}", output::style_dim(&replay_line));
                last_heading = None;
                block_idx += 1;
                continue;
            }

            if !replay_cleaned {
                session.remove_env("WB_REPLAY");
                session.unset_env_in_sessions("WB_REPLAY");
                replay_cleaned = true;
            }

            let block_heading = last_heading.take();
            let live_skip = if !block_selected(&selection_range, &selection_restricts, block_idx) {
                Some(selection_skip_decision())
            } else if spec.skip_execution {
                Some(no_run_skip_decision())
            } else {
                conditional_skip_decision(
                    spec.when.as_deref(),
                    spec.skip_if.as_deref(),
                    &parser::resolved_env(session.env()),
                )
            };
            if let Some(skip) = live_skip {
                if !quiet {
                    let label = if skip.kind == "no_run" {
                        format!("  ⊘ skipped {{no-run}} [browser] (L{})", spec.line_number)
                    } else {
                        format!(
                            "  ⊘ skipped [browser] (L{}) — {}",
                            spec.line_number, skip.reason
                        )
                    };
                    eprintln!("{}", output::style_dim(&label));
                }
                if let Some(ref cb) = cb {
                    if should_emit_skip_callback(spec.silent, workflow_payload.as_ref()) {
                        cb.step_skipped(
                            file,
                            checkpoint_id.as_deref(),
                            block_idx,
                            step_id,
                            "browser",
                            block_heading.as_deref(),
                            spec.line_number,
                            block_idx + 1,
                            block_count,
                            &skip.kind,
                            skip.expression.as_deref(),
                            &skip.reason,
                            &include_stack,
                            workflow_payload.as_ref(),
                        );
                    }
                }
                if let (Some(ref mut c), Some(ref ckpt_id)) = (&mut ckpt, &checkpoint_id) {
                    c.add_skip(checkpoint::SavedSkip {
                        block_index: block_idx,
                        step_id: step_id.map(|s| s.to_string()),
                        language: "browser".to_string(),
                        line_number: spec.line_number,
                        heading: block_heading.clone(),
                        kind: skip.kind,
                        expression: skip.expression,
                        reason: skip.reason,
                        code_hash: Some(checkpoint::hash_code(&spec.raw)),
                    });
                    c.next_step_id = steps.get(c.next_block).map(|s| s.id.0.clone());
                    if let Err(e) = checkpoint::save(ckpt_id, c) {
                        log_warn!("warning: checkpoint: {}", e);
                    }
                }
                block_idx += 1;
                continue;
            }

            if !quiet {
                let session_tag = spec.session.as_deref().unwrap_or("-");
                let preview = format!("session={} verbs={}", session_tag, spec.verbs.len());
                output::print_block_header(
                    block_heading.as_deref(),
                    "browser",
                    spec.line_number,
                    Some(&preview),
                );
            }

            let slice_ctx = sidecar::SliceCallbackContext {
                cb: cb.as_ref(),
                workbook: file,
                checkpoint_id: checkpoint_id.as_deref(),
                block_index: block_idx,
                heading: block_heading.as_deref(),
                line_number: spec.line_number,
                completed: block_idx + 1,
                total: block_count,
                include_chain: &include_stack,
                step_id,
                workflow: workflow_payload.as_ref(),
            };
            // Take a one-shot restore, if the resume path handed us one earlier.
            let restore = browser_restore.take();
            let (mut result, pause_info) = match prepare_browser_spec(spec, artifacts.dir()) {
                Ok(prepared_spec) => session.execute_browser_slice(
                    &prepared_spec,
                    block_idx,
                    &slice_ctx,
                    restore.as_ref(),
                ),
                Err(e) => (
                    executor::BlockResult {
                        block_index: block_idx,
                        language: "browser".to_string(),
                        stdout: String::new(),
                        stderr: e,
                        exit_code: 1,
                        duration: std::time::Duration::ZERO,
                        error_type: Some("browser_verb_failed".to_string()),
                        stdout_partial: false,
                        stderr_partial: false,
                    },
                    None,
                ),
            };
            let current_outputs = capture_outputs_for_result(&mut result, false);
            if !current_outputs.is_empty() {
                let key = step_key(step_id, block_idx);
                step_outputs::merge_step_outputs(&mut run_outputs, &key, &current_outputs);
                // Export outputs into the session env (WB_OUT_<name>) so later
                // cells' `{when=...}` / `{skip_if=...}` can branch on a value
                // this slice produced (e.g. an `eval` of login state).
                step_outputs::export_to_session(&mut session, &current_outputs);
                if let Err(e) = step_outputs::write_outputs_file(&outputs_path, &run_outputs) {
                    log_warn!("warning: outputs file: {}", e);
                }
                if let (Some(ref mut c), Some(ref ckpt_id)) = (&mut ckpt, &checkpoint_id) {
                    c.add_outputs(&key, &raw_outputs(&current_outputs));
                    let _ = checkpoint::save(ckpt_id, c);
                }
            }
            let callback_outputs = if current_outputs.is_empty() {
                None
            } else {
                Some(step_outputs::callback_outputs(&current_outputs))
            };
            let new_artifacts = artifacts.sync();
            artifacts.record(step_id, &new_artifacts);

            // Emit step.artifact_saved for each artifact this slice produced,
            // before step.complete (or before the pause path exits). Silent
            // slices suppress the events; see the code-block site above for
            // the same pattern.
            if let Some(ref cb) = cb {
                if !spec.silent {
                    for art in &new_artifacts {
                        cb.step_artifact_saved(
                            file,
                            checkpoint_id.as_deref(),
                            block_idx,
                            "browser",
                            block_heading.as_deref(),
                            spec.line_number,
                            block_idx + 1,
                            block_count,
                            &art.filename,
                            &art.path.to_string_lossy(),
                            art.bytes,
                            art.content_type,
                            art.label.as_deref(),
                            art.description.as_deref(),
                            &include_stack,
                            step_id,
                            workflow_payload.as_ref(),
                        );
                    }
                }
            }

            if let Some(pause) = pause_info {
                // pause_browser_slice diverges via std::process::exit. For a
                // human-in-the-loop browser pause we must keep the remote
                // browser session alive, so suspend the sidecar instead of
                // dropping it through the normal shutdown/release path.
                session.suspend_browser_sidecar();
                drop(session);
                pause_browser_slice(
                    spec,
                    section_idx,
                    &checkpoint_id,
                    ckpt.as_mut(),
                    file,
                    block_idx,
                    cb.as_ref(),
                    block_heading.as_deref(),
                    pause,
                    &include_stack,
                    &frame_starts,
                    step_id,
                    workflow_payload.as_ref(),
                    &steps,
                );
                // Unreachable — pause_browser_slice exits.
            }

            let success = result.success();

            let status_line = format!(
                "{} [{}/{}] browser ({:.1}s)",
                if success { "✓" } else { "✗" },
                block_idx + 1,
                block_count,
                result.duration.as_secs_f64()
            );
            if success {
                eprintln!("{}", output::style_ok(&status_line));
            } else {
                eprintln!("{}", output::style_fail(&status_line));
            }

            if let Some(ref cb) = cb {
                if !spec.silent {
                    cb.step_complete(
                        &result,
                        block_idx + 1,
                        block_count,
                        file,
                        checkpoint_id.as_deref(),
                        block_heading.as_deref(),
                        spec.line_number,
                        &include_stack,
                        step_id,
                        callback_outputs.as_ref(),
                        workflow_payload.as_ref(),
                    );
                }
            }

            if bail && !success {
                if let (Some(ref cb), Some(ref ckpt_id)) = (&cb, &checkpoint_id) {
                    cb.checkpoint_failed(
                        &result,
                        block_idx,
                        block_count,
                        file,
                        ckpt_id,
                        block_heading.as_deref(),
                        spec.line_number,
                        &include_stack,
                        step_id,
                        workflow_payload.as_ref(),
                    );
                }

                emit_finish_for_stack(
                    cb.as_ref(),
                    file,
                    checkpoint_id.as_deref(),
                    &mut include_stack,
                    &mut frame_starts,
                    "failed",
                );

                if let (Some(ref mut c), Some(ref ckpt_id)) = (&mut ckpt, &checkpoint_id) {
                    c.mark_failed();
                    let _ = checkpoint::save(ckpt_id, c);
                }
                results.push(result);
                break;
            }

            if let (Some(ref mut c), Some(ref ckpt_id)) = (&mut ckpt, &checkpoint_id) {
                c.add_result(
                    &result,
                    spec.line_number,
                    block_heading.as_deref(),
                    &spec.raw,
                    step_id,
                );
                c.next_step_id = steps.get(c.next_block).map(|s| s.id.0.clone());
                if let Err(e) = checkpoint::save(ckpt_id, c) {
                    log_warn!("warning: checkpoint: {}", e);
                }
            }

            // If this slice was resumed from a pending descriptor (browser
            // pause), clean up now that it completed.
            if restore.is_some() {
                if let Some(ref id) = checkpoint_id {
                    let _ = pending::delete(id);
                }
                std::env::remove_var("WB_BROWSER_RESUME_SIGNAL");
            }

            results.push(result);
            block_idx += 1;
        }
    }

    // Mark complete if all blocks ran
    if let (Some(ref mut c), Some(ref ckpt_id)) = (&mut ckpt, &checkpoint_id) {
        if c.status == checkpoint::CheckpointStatus::InProgress {
            c.mark_complete();
            let _ = checkpoint::save(ckpt_id, c);
        }
    }

    let total_duration = start.elapsed();
    let passed = results.iter().filter(|r| r.success()).count();
    let failed = results.iter().filter(|r| !r.success()).count();

    // Persist the source-hash cache (#18) so the next `--cache` run can skip
    // unchanged successful blocks.
    if let (Some(id), Some(store)) = (&cache_id, &cache_store) {
        if let Err(e) = store.save(id) {
            log_warn!("warning: could not save cache '{}': {}", id, e);
        }
    }

    // Callback: run complete
    if let Some(ref cb) = cb {
        cb.run_complete(
            passed,
            failed,
            block_count,
            total_duration.as_millis() as u64,
            file,
            checkpoint_id.as_deref(),
        );
    }

    let summary = output::RunSummary {
        source_file: file.to_string(),
        run_id: run_id.clone(),
        total_blocks: block_count,
        passed,
        failed,
        total_duration,
        results,
    };

    output::print_summary(&summary);
    write_run_output(
        &workbook,
        &summary,
        output_format,
        output_path.as_deref(),
        stdout_output,
    );

    if failed > 0 {
        // std::process::exit skips Drop, which would orphan the browser
        // sidecar (Sidecar::Drop sends the shutdown frame + bounded-wait
        // for recording flush + browser.close). Drop session first so the
        // browser context actually closes on --bail and end-of-run failures.
        drop(session);
        std::process::exit(1);
    }
}

fn inspect_workbook(file: &str) {
    let content = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}: {}", file, e);
            std::process::exit(1);
        }
    };

    let workbook = parse_and_resolve(&content, file);

    if let Some(ref title) = workbook.frontmatter.title {
        println!("title: {}", title);
    }
    if let Some(ref runtime) = workbook.frontmatter.runtime {
        println!("runtime: {}", runtime);
    }
    if let Some(ref venv) = workbook.frontmatter.venv {
        println!("venv: {}", venv);
    }
    if let Some(ref vars) = workbook.frontmatter.vars {
        println!(
            "vars: {}",
            vars.keys().cloned().collect::<Vec<_>>().join(", ")
        );
    }
    if let Some(ref redact) = workbook.frontmatter.redact {
        println!("redact: {}", redact.join(", "));
    }
    if workbook.frontmatter.secrets.is_some() {
        println!("secrets: configured");
    }

    // Show requires/sandbox config
    if let Some(ref req) = workbook.frontmatter.requires {
        println!("sandbox: {}", req.sandbox);
        if !req.apt.is_empty() {
            println!("  apt: {}", req.apt.join(", "));
        }
        if !req.pip.is_empty() {
            println!("  pip: {}", req.pip.join(", "));
        }
        if !req.node.is_empty() {
            println!("  node: {}", req.node.join(", "));
        }
        if let Some(ref df) = req.dockerfile {
            println!("  dockerfile: {}", df);
        }
        println!("  image: {}", sandbox::image_tag(req));
    }

    // Show exec mappings
    if let Some(ref exec) = workbook.frontmatter.exec {
        match exec {
            parser::ExecConfig::Global(s) => println!("exec: {}", s),
            parser::ExecConfig::PerLanguage(map) => {
                for (lang, cmd) in map {
                    println!("exec.{}: {}", lang, cmd);
                }
            }
        }
    }

    println!();
    let mut idx = 0;
    for section in &workbook.sections {
        if let parser::Section::Code(block) = section {
            idx += 1;
            let lang = block.language.to_lowercase();

            // Resolve what actually runs this block
            let resolved = match &workbook.frontmatter.exec {
                Some(parser::ExecConfig::Global(prefix)) => {
                    format!("{} {}", prefix, default_program(&lang))
                }
                Some(parser::ExecConfig::PerLanguage(map)) => {
                    let normalized = normalize_block_language(&lang);
                    match map.get(normalized) {
                        Some(cmd) => cmd.clone(),
                        None => default_program(&lang).to_string(),
                    }
                }
                None => default_program(&lang).to_string(),
            };

            let preview: String = block
                .code
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(50)
                .collect();
            let flag_tag = flag_annotation(block.skip_execution, block.silent);
            println!(
                "  {}. [{}]{} L{} -> {} — {}",
                idx, block.language, flag_tag, block.line_number, resolved, preview
            );
        } else if let parser::Section::Wait(spec) = section {
            let mut parts = Vec::new();
            if let Some(ref k) = spec.kind {
                parts.push(format!("kind={}", k));
            }
            if let Some(ref b) = spec.bind {
                let bind_str = match b {
                    parser::BindSpec::Single(s) => s.clone(),
                    parser::BindSpec::Multiple(v) => format!("[{}]", v.join(", ")),
                };
                parts.push(format!("bind={}", bind_str));
            }
            if let Some(ref t) = spec.timeout {
                parts.push(format!("timeout={}", t));
            }
            if let Some(ref ot) = spec.on_timeout {
                parts.push(format!("on_timeout={}", ot));
            }
            let detail = if parts.is_empty() {
                String::new()
            } else {
                format!(" {}", parts.join(" "))
            };
            println!("  \u{23f8} wait{} (L{})", detail, spec.line_number);
        } else if let parser::Section::Browser(spec) = section {
            idx += 1;
            let session_tag = spec.session.as_deref().unwrap_or("-");
            let flag_tag = flag_annotation(spec.skip_execution, spec.silent);
            println!(
                "  {}. [browser]{} L{} -> wb-browser-runtime — session={} verbs={}",
                idx,
                flag_tag,
                spec.line_number,
                session_tag,
                spec.verbs.len()
            );
        }
    }

    if idx == 0 {
        println!("  (no executable blocks)");
    }
}

/// Machine-readable inspect output. Same semantics as `inspect_workbook` but
/// emits a JSON document agents can parse directly instead of grepping prose.
/// Shape (stable within a major version):
///   {
///     "source": "...",
///     "frontmatter": { "title": "...", "runtime": "...", "sandbox": { ... } },
///     "blocks": [
///       { "index": 1, "kind": "code"|"wait"|"browser",
///         "language": "bash", "line": 12, "heading": "Step Two",
///         "flags": { "no_run": false, "silent": false },
///         /* code-only */    "resolved_exec": "bash",
///         /* wait-only */    "kind_name": "email", "bind": "otp_code", "timeout": "5m",
///         /* browser-only */ "session": "main", "verb_count": 3 }
///     ]
///   }
fn inspect_workbook_json(file: &str) {
    let content = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}: {}", file, e);
            std::process::exit(1);
        }
    };
    let workbook = parse_and_resolve(&content, file);

    let fm = &workbook.frontmatter;
    let sandbox_obj = fm.requires.as_ref().map(|req| {
        serde_json::json!({
            "kind": req.sandbox,
            "apt": req.apt,
            "pip": req.pip,
            "node": req.node,
            "dockerfile": req.dockerfile,
            "image": sandbox::image_tag(req),
        })
    });

    let mut blocks = Vec::new();
    let mut idx: usize = 0;
    let steps = workbook.build_steps();
    let mut step_idx: usize = 0;
    for section in &workbook.sections {
        match section {
            parser::Section::Code(block) => {
                idx += 1;
                let lang = block.language.to_lowercase();
                let resolved = match &fm.exec {
                    Some(parser::ExecConfig::Global(prefix)) => {
                        format!("{} {}", prefix, default_program(&lang))
                    }
                    Some(parser::ExecConfig::PerLanguage(map)) => {
                        let normalized = normalize_block_language(&lang);
                        match map.get(normalized) {
                            Some(cmd) => cmd.clone(),
                            None => default_program(&lang).to_string(),
                        }
                    }
                    None => default_program(&lang).to_string(),
                };
                let step_id = steps.get(step_idx).map(|s| s.id.0.clone());
                step_idx += 1;
                blocks.push(serde_json::json!({
                    "index": idx,
                    "kind": "code",
                    "step_id": step_id,
                    "language": block.language,
                    "line": block.line_number,
                    "flags": {
                        "no_run": block.skip_execution,
                        "silent": block.silent,
                    },
                    "resolved_exec": resolved,
                }));
            }
            parser::Section::Wait(spec) => {
                let bind_val = spec.bind.as_ref().map(|b| match b {
                    parser::BindSpec::Single(s) => serde_json::Value::String(s.clone()),
                    parser::BindSpec::Multiple(v) => serde_json::Value::Array(
                        v.iter()
                            .map(|s| serde_json::Value::String(s.clone()))
                            .collect(),
                    ),
                });
                blocks.push(serde_json::json!({
                    "kind": "wait",
                    "line": spec.line_number,
                    "kind_name": spec.kind,
                    "bind": bind_val,
                    "timeout": spec.timeout,
                    "on_timeout": spec.on_timeout,
                }));
            }
            parser::Section::Browser(spec) => {
                idx += 1;
                let step_id = steps.get(step_idx).map(|s| s.id.0.clone());
                step_idx += 1;
                blocks.push(serde_json::json!({
                    "index": idx,
                    "kind": "browser",
                    "step_id": step_id,
                    "language": "browser",
                    "line": spec.line_number,
                    "flags": {
                        "no_run": spec.skip_execution,
                        "silent": spec.silent,
                    },
                    "session": spec.session,
                    "verb_count": spec.verbs.len(),
                }));
            }
            parser::Section::Text(_) => {}
            parser::Section::Expect(spec) => {
                blocks.push(serde_json::json!({
                    "index": serde_json::Value::Null,
                    "kind": "expect",
                    "line": spec.line_number,
                    "assertions": spec.assertions.len(),
                }));
            }
            parser::Section::Include(_) => {
                unreachable!(
                    "Section::Include must be resolved by parser::resolve_includes before inspect"
                )
            }
            parser::Section::IncludeEnter(frame) => {
                blocks.push(serde_json::json!({
                    "kind": "include_enter",
                    "step_id": frame.id,
                    "step_title": frame.title,
                }));
            }
            parser::Section::IncludeExit(frame) => {
                blocks.push(serde_json::json!({
                    "kind": "include_exit",
                    "step_id": frame.id,
                }));
            }
        }
    }

    let out = serde_json::json!({
        "source": file,
        "frontmatter": {
            "title": fm.title,
            "runtime": fm.runtime,
            "venv": fm.venv,
            "vars": fm.vars.as_ref().map(|v| v.keys().cloned().collect::<Vec<_>>()),
            "redact": fm.redact,
            "secrets": fm.secrets.is_some(),
            "sandbox": sandbox_obj,
            "workflow": fm.workflow,
        },
        "blocks": blocks,
        "executable_count": workbook.code_block_count(),
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

/// Render `{no-run}` / `{silent}` badges next to the language tag in `wb inspect`.
fn flag_annotation(skip_execution: bool, silent: bool) -> String {
    match (skip_execution, silent) {
        (true, _) => " {no-run}".to_string(),
        (false, true) => " {silent}".to_string(),
        _ => String::new(),
    }
}

fn normalize_block_language(lang: &str) -> &str {
    match lang {
        "python" | "python3" | "py" => "python",
        "bash" | "shell" => "bash",
        "node" | "javascript" | "js" => "node",
        "ruby" | "rb" => "ruby",
        other => other,
    }
}

fn default_program(lang: &str) -> &str {
    match lang {
        "python" | "python3" | "py" => "python3",
        "bash" | "shell" => "bash",
        "sh" => "sh",
        "zsh" => "zsh",
        "node" | "javascript" | "js" => "node",
        "ruby" | "rb" => "ruby",
        "perl" => "perl",
        "php" => "php",
        "lua" => "lua",
        "r" => "Rscript",
        "swift" => "swift",
        "go" => "go",
        _ => "bash",
    }
}

use exit_codes::EXIT_PAUSED;

/// Write the operator's action choice to `<artifacts_dir>/pause_result.json`
/// so downstream cells can branch on it with a plain file read. Shape is
/// always `{"value": <choice>}` regardless of how the choice came in via
/// `wb resume`:
///   - `--value X`       → writes `{"value": "X"}`
///   - `--signal {value: X}` → writes `{"value": X}` (value can be any JSON)
///   - any other shape   → writes `{"value": <whole signal>}`
///
/// Best-effort: failures log a warning but do not abort the resume — the
/// downstream cell can still check `test -s $WB_ARTIFACTS_DIR/pause_result.json`
/// and fall through to a default.
fn write_pause_result(dir: &std::path::Path, signal: Option<&serde_json::Value>) {
    let path = dir.join("pause_result.json");
    let value = match signal {
        Some(serde_json::Value::Object(obj)) => {
            // Prefer an explicit `value` key so the shape matches single-bind
            // resumes where the operator just sent `--value X`.
            obj.get("value")
                .cloned()
                .unwrap_or_else(|| serde_json::Value::Object(obj.clone()))
        }
        Some(v) => v.clone(),
        None => serde_json::Value::Null,
    };
    let payload = serde_json::json!({ "value": value });
    let serialized = match serde_json::to_string_pretty(&payload) {
        Ok(s) => s,
        Err(e) => {
            log_warn!("warning: pause_result.json serialize: {}", e);
            return;
        }
    };
    if let Err(e) = std::fs::write(&path, serialized) {
        log_warn!(
            "warning: pause_result.json write ({}): {}",
            path.display(),
            e
        );
    }
}

fn prepare_browser_spec(
    spec: &parser::BrowserSliceSpec,
    artifacts_dir: &Path,
) -> Result<parser::BrowserSliceSpec, String> {
    let mut prepared = spec.clone();
    let mut verbs = Vec::with_capacity(spec.verbs.len());
    for verb in &spec.verbs {
        if let Some(args) = announce_artifact_args(verb)? {
            write_artifact_sidecar(artifacts_dir, &args)?;
        } else {
            verbs.push(verb.clone());
        }
    }
    prepared.verbs = verbs;
    Ok(prepared)
}

struct AnnounceArtifactArgs {
    path: String,
    label: String,
    description: Option<String>,
}

fn announce_artifact_args(
    verb: &serde_yaml::Value,
) -> Result<Option<AnnounceArtifactArgs>, String> {
    let Some(map) = verb.as_mapping() else {
        return Ok(None);
    };
    let key = serde_yaml::Value::String("announce_artifact".to_string());
    let Some(value) = map.get(&key) else {
        return Ok(None);
    };
    let Some(args) = value.as_mapping() else {
        return Err("announce_artifact: expected mapping".to_string());
    };
    let path = yaml_string(args, "path")
        .ok_or_else(|| "announce_artifact: `path` is required".to_string())?;
    let label = yaml_string(args, "label")
        .ok_or_else(|| "announce_artifact: `label` is required".to_string())?;
    let description = yaml_string(args, "description");
    Ok(Some(AnnounceArtifactArgs {
        path,
        label,
        description,
    }))
}

fn yaml_string(map: &serde_yaml::Mapping, key: &str) -> Option<String> {
    map.get(serde_yaml::Value::String(key.to_string()))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn write_artifact_sidecar(dir: &Path, args: &AnnounceArtifactArgs) -> Result<(), String> {
    let rel = Path::new(&args.path);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(format!(
            "announce_artifact: path '{}' must stay inside WB_ARTIFACTS_DIR",
            args.path
        ));
    }
    let artifact_path = dir.join(rel);
    let mut sidecar_os = artifact_path.as_os_str().to_os_string();
    sidecar_os.push(".meta.json");
    let sidecar = PathBuf::from(sidecar_os);
    if let Some(parent) = sidecar.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("announce_artifact: create {}: {}", parent.display(), e))?;
    }
    let mut payload = serde_json::Map::new();
    payload.insert(
        "label".to_string(),
        serde_json::Value::String(args.label.clone()),
    );
    if let Some(description) = args.description.as_ref() {
        payload.insert(
            "description".to_string(),
            serde_json::Value::String(description.clone()),
        );
    }
    let bytes = serde_json::to_vec_pretty(&serde_json::Value::Object(payload))
        .map_err(|e| format!("announce_artifact: serialize sidecar: {}", e))?;
    atomic_io::write_secret_file(&sidecar, &bytes)
        .map_err(|e| format!("announce_artifact: write {}: {}", sidecar.display(), e))?;
    Ok(())
}

/// Fire `step.finished` for every active include frame, deepest-first. Drains
/// the stack. Used on bail-failure so the run-page timeline shows each parent
/// include closing with the same terminal outcome as the failing block.
fn emit_finish_for_stack(
    cb: Option<&callback::CallbackConfig>,
    file: &str,
    checkpoint_id: Option<&str>,
    include_stack: &mut Vec<parser::IncludeFrame>,
    frame_starts: &mut Vec<Instant>,
    outcome: &str,
) {
    while let (Some(frame), Some(start)) = (include_stack.pop(), frame_starts.pop()) {
        let parent_id = include_stack.last().map(|f| f.id.clone());
        if let Some(cb) = cb {
            cb.step_finished(
                file,
                checkpoint_id,
                &frame,
                parent_id.as_deref(),
                start.elapsed().as_millis() as u64,
                outcome,
            );
        }
    }
}

/// Fire `step.finished(outcome="paused")` for every active frame without
/// consuming the stack. The run-page timeline flips the chain into a paused
/// state; on resume, `step.started` re-fires for the same chain so the
/// timeline unfreezes. Used by pause paths that diverge via `process::exit`
/// (where mutating the stack buys nothing — the process is gone).
fn emit_paused_finish_snapshot(
    cb: Option<&callback::CallbackConfig>,
    file: &str,
    checkpoint_id: Option<&str>,
    include_chain: &[parser::IncludeFrame],
    frame_starts: &[Instant],
) {
    let Some(cb) = cb else { return };
    for i in (0..include_chain.len()).rev() {
        let frame = &include_chain[i];
        let parent_id = if i > 0 {
            Some(include_chain[i - 1].id.as_str())
        } else {
            None
        };
        let duration = frame_starts
            .get(i)
            .map(|s| s.elapsed().as_millis() as u64)
            .unwrap_or(0);
        cb.step_finished(file, checkpoint_id, frame, parent_id, duration, "paused");
    }
}

#[allow(clippy::too_many_arguments)]
fn pause_for_signal(
    spec: &parser::WaitSpec,
    section_idx: usize,
    checkpoint_id: &Option<String>,
    ckpt: Option<&mut checkpoint::Checkpoint>,
    file: &str,
    block_idx: usize,
    next_step_id: Option<&str>,
    _block_count: usize,
    _elapsed: std::time::Duration,
    _results: &[executor::BlockResult],
    cb: Option<&callback::CallbackConfig>,
    include_chain: &[parser::IncludeFrame],
    frame_starts: &[Instant],
    workflow: Option<&callback::WorkflowPayload>,
) -> ! {
    let id = match checkpoint_id.as_deref() {
        Some(id) => id,
        None => {
            eprintln!(
                "error: `wait` blocks require --checkpoint <id> to pause and resume. (L{})",
                spec.line_number
            );
            std::process::exit(1);
        }
    };

    // Save checkpoint in Paused state. bound_vars and waits_completed persist.
    if let Some(c) = ckpt {
        c.next_block = block_idx;
        c.next_step_id = next_step_id.map(|s| s.to_string());
        // Persist the callback sequence high-water mark so resume continues
        // X-WB-Sequence monotonically instead of restarting at 0.
        if let Some(cb) = cb {
            c.callback_seq = cb.seq_value();
        }
        c.mark_paused();
        if let Err(e) = checkpoint::save(id, c) {
            log_warn!("warning: checkpoint: {}", e);
        }
    }

    // Write pending-signal descriptor next to the checkpoint.
    let mut spec_with_idx = spec.clone();
    spec_with_idx.section_index = section_idx;
    let cb_for_desc = cb.map(|c| (c.url.as_str(), c.secret.as_deref()));
    let desc = pending::build(
        id,
        file,
        block_idx,
        next_step_id,
        &spec_with_idx,
        cb_for_desc,
    );
    if let Err(e) = pending::save(id, &desc) {
        log_warn!("warning: pending descriptor: {}", e);
    }

    let bind_label = spec
        .bind
        .as_ref()
        .map(|b| match b {
            parser::BindSpec::Single(s) => s.clone(),
            parser::BindSpec::Multiple(v) => v.join(","),
        })
        .unwrap_or_else(|| "-".to_string());

    eprintln!(
        "{}",
        output::style_bold(&format!(
            "⏸  paused at L{} — kind={} bind={} (checkpoint: {})",
            spec.line_number,
            spec.kind.as_deref().unwrap_or("-"),
            bind_label,
            id,
        ))
    );
    let desc_path = pending::descriptor_path(id);
    eprintln!("   pending: {}", desc_path.display());
    if let Some(ref to) = desc.timeout_at {
        eprintln!("   timeout_at: {}", to);
    }
    eprintln!("   resume:  wb resume {} --signal <payload.json>", id);

    // Fire workbook.paused callback so agents know we're waiting.
    if let Some(cb) = cb {
        let bind_names: Option<Vec<String>> = spec.bind.as_ref().map(|b| match b {
            parser::BindSpec::Single(s) => vec![s.clone()],
            parser::BindSpec::Multiple(v) => v.clone(),
        });
        cb.workbook_paused(
            file,
            id,
            spec.kind.as_deref(),
            bind_names.as_deref(),
            desc.timeout_at.as_deref(),
            include_chain,
            workflow,
        );
        // Close active include frames with outcome=paused so the run-page
        // timeline flips them into a pause state. On resume, step.started
        // re-fires for the same chain (via emitted_resume_chain).
        emit_paused_finish_snapshot(Some(cb), file, Some(id), include_chain, frame_starts);
    }

    std::process::exit(EXIT_PAUSED);
}

/// Pause a browser slice mid-run: persist sidecar state + pending descriptor,
/// fire callback events, and exit EXIT_PAUSED. Mirrors `pause_for_signal` for
/// the `wait` flow.
#[allow(clippy::too_many_arguments)]
fn pause_browser_slice(
    spec: &parser::BrowserSliceSpec,
    _section_idx: usize,
    checkpoint_id: &Option<String>,
    ckpt: Option<&mut checkpoint::Checkpoint>,
    file: &str,
    block_idx: usize,
    cb: Option<&callback::CallbackConfig>,
    _heading: Option<&str>,
    pause: sidecar::PauseInfo,
    include_chain: &[parser::IncludeFrame],
    frame_starts: &[Instant],
    step_id: Option<&str>,
    workflow: Option<&callback::WorkflowPayload>,
    steps: &[step_ir::Step],
) -> ! {
    let id = match checkpoint_id.as_deref() {
        Some(id) => id,
        None => {
            eprintln!(
                "error: browser slice paused but no --checkpoint was set; slices with pauses require --checkpoint to resume. (L{})",
                spec.line_number
            );
            std::process::exit(1);
        }
    };

    // F7b: reject navigation actions that point at a non-existent step before
    // we persist the paused state, so the operator never sees a dead button.
    if let Err(bad) = validate_pause_action_targets(&pause.actions, steps) {
        eprintln!(
            "error: pause_for_human action target '{}' (L{}) does not match any step id in this workbook",
            bad, spec.line_number
        );
        std::process::exit(exit_codes::EXIT_WORKBOOK_INVALID);
    }

    if let Some(c) = ckpt {
        c.next_block = block_idx;
        c.next_step_id = step_id.map(|s| s.to_string());
        if let Some(cb) = cb {
            c.callback_seq = cb.seq_value();
        }
        c.mark_paused();
        if let Err(e) = checkpoint::save(id, c) {
            log_warn!("warning: checkpoint: {}", e);
        }
    }

    let cb_for_desc = cb.map(|c| (c.url.as_str(), c.secret.as_deref()));
    let desc =
        pending::build_for_browser_pause(id, file, block_idx, step_id, spec, &pause, cb_for_desc);
    if let Err(e) = pending::save(id, &desc) {
        log_warn!("warning: pending descriptor: {}", e);
    }

    eprintln!(
        "{}",
        output::style_bold(&format!(
            "⏸  browser slice paused at L{} — {} (checkpoint: {})",
            spec.line_number,
            pause.reason.as_deref().unwrap_or("slice.paused"),
            id,
        ))
    );
    if let Some(ref url) = pause.resume_url {
        eprintln!("   resume_url: {}", url);
    }
    let desc_path = pending::descriptor_path(id);
    eprintln!("   pending: {}", desc_path.display());
    eprintln!("   resume:  wb resume {} [--signal <payload.json>]", id);

    // Backwards-compat: fire workbook.paused so consumers already subscribed
    // to that event see the run as paused.
    if let Some(cb) = cb {
        cb.workbook_paused(
            file,
            id,
            pause.reason.as_deref().or(Some("browser.slice_paused")),
            None,
            desc.timeout_at.as_deref(),
            include_chain,
            workflow,
        );
        // The sidecar already emitted the granular `step.paused` lifecycle
        // event before returning `PauseInfo`; do not duplicate it here.
        // Same bubble-up as pause_for_signal: close active frames with
        // outcome=paused so the run-page timeline freezes.
        emit_paused_finish_snapshot(Some(cb), file, Some(id), include_chain, frame_starts);
    }

    std::process::exit(EXIT_PAUSED);
}

/// Resolve an env-file path relative to the workbook's directory.
/// Absolute paths are returned as-is.
fn resolve_env_file_path(path: &str, workbook_dir: &str) -> String {
    if Path::new(path).is_absolute() {
        path.to_string()
    } else {
        let resolved = Path::new(workbook_dir).join(path);
        resolved.to_string_lossy().to_string()
    }
}

fn run_setup(setup: &parser::SetupConfig, base_dir: &str) -> Result<(), String> {
    let work_dir = setup
        .dir()
        .map(|d| {
            if Path::new(d).is_absolute() {
                d.to_string()
            } else {
                Path::new(base_dir).join(d).to_string_lossy().to_string()
            }
        })
        .unwrap_or_else(|| base_dir.to_string());

    for cmd in setup.commands() {
        let status = std::process::Command::new("sh")
            .args(["-c", cmd])
            .current_dir(&work_dir)
            .status()
            .map_err(|e| format!("setup '{}': {}", cmd, e))?;

        if !status.success() {
            return Err(format!(
                "setup '{}' failed (exit {})",
                cmd,
                status.code().unwrap_or(-1)
            ));
        }
    }
    Ok(())
}

fn build_secrets_config(
    frontmatter_secrets: &Option<parser::SecretsConfig>,
    cli_provider: Option<String>,
    cli_project: Option<String>,
    cli_command: Option<String>,
) -> Option<parser::SecretsConfig> {
    if let Some(provider) = cli_provider {
        return Some(parser::SecretsConfig::Single(parser::SecretProvider {
            provider,
            project: cli_project,
            command: cli_command,
            keys: None,
        }));
    }
    frontmatter_secrets.clone()
}

fn transform_workbook(file: &str) {
    let content = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}: {}", file, e);
            std::process::exit(1);
        }
    };

    let workbook = parse_and_resolve(&content, file);
    let existing_vars = workbook.frontmatter.vars.clone().unwrap_or_default();

    // Scan code blocks for {{key}} patterns
    let mut referenced = std::collections::BTreeSet::new();
    for section in &workbook.sections {
        if let parser::Section::Code(block) = section {
            let mut rest = block.code.as_str();
            while let Some(start) = rest.find("{{") {
                if let Some(end) = rest[start + 2..].find("}}") {
                    let key = rest[start + 2..start + 2 + end].trim();
                    if !key.is_empty()
                        && key
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                    {
                        referenced.insert(key.to_string());
                    }
                    rest = &rest[start + 2 + end + 2..];
                } else {
                    break;
                }
            }
        }
    }

    if referenced.is_empty() {
        eprintln!("no {{{{variables}}}} found in code blocks");
        std::process::exit(0);
    }

    let missing: Vec<&String> = referenced
        .iter()
        .filter(|k| !existing_vars.contains_key(k.as_str()))
        .collect();

    let unused: Vec<String> = existing_vars
        .keys()
        .filter(|k| !referenced.contains(*k))
        .cloned()
        .collect();

    println!("vars:");
    for key in &referenced {
        match existing_vars.get(key.as_str()) {
            Some(val) if !val.is_empty() => println!("  {}: {}", key, val),
            _ => println!("  {}: \"\"", key),
        }
    }

    if !missing.is_empty() {
        eprintln!(
            "\nundefined: {}",
            missing
                .iter()
                .map(|k| k.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    if !unused.is_empty() {
        eprintln!("\nunused: {}", unused.join(", "));
    }
}

// ─── wait/pending/resume commands ─────────────────────────────────────

// ─── containers subcommand ──────────────────────────────────────────

fn cmd_containers_build(path: &str, json: bool) -> WbExit {
    let files = if Path::new(path).is_dir() {
        collect_workbooks(path, "a-z")
    } else {
        vec![path.to_string()]
    };

    if files.is_empty() {
        if json {
            print_json(&serde_json::json!({
                "built": 0, "cached": 0, "errors": 0, "results": [],
            }));
        } else {
            eprintln!("no .md files found in {}", path);
        }
        return WbExit::Success;
    }

    let mut built = 0;
    let mut skipped = 0;
    let mut errors = 0;
    let mut results: Vec<serde_json::Value> = Vec::new();

    for file in &files {
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                if json {
                    results.push(serde_json::json!({
                        "file": file, "status": "error", "detail": e.to_string(),
                    }));
                } else {
                    eprintln!("  error: {}: {}", file, e);
                }
                errors += 1;
                continue;
            }
        };

        let workbook = parse_and_resolve(&content, file);
        let requires = match workbook.frontmatter.requires {
            Some(ref r) => r,
            None => {
                if json {
                    results.push(serde_json::json!({ "file": file, "status": "skipped" }));
                }
                skipped += 1;
                continue;
            }
        };

        let filename = Path::new(file)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| file.clone());

        let tag = sandbox::image_tag(requires);
        if sandbox::image_exists(&tag) {
            if json {
                results.push(serde_json::json!({
                    "file": file, "status": "cached", "tag": tag,
                }));
            } else {
                eprintln!("  ✓ {} (cached: {})", filename, tag);
            }
            skipped += 1;
            continue;
        }

        let workbook_dir = Path::new(file)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());

        match sandbox::build_image(requires, &workbook_dir) {
            Ok(t) => {
                if json {
                    results.push(serde_json::json!({
                        "file": file, "status": "built", "tag": t,
                    }));
                } else {
                    eprintln!("  ✓ {} -> {}", filename, t);
                }
                built += 1;
            }
            Err(e) => {
                if json {
                    results.push(serde_json::json!({
                        "file": file, "status": "error", "detail": e.to_string(),
                    }));
                } else {
                    eprintln!("  ✗ {} — {}", filename, e);
                }
                errors += 1;
            }
        }
    }

    if json {
        print_json(&serde_json::json!({
            "built": built, "cached": skipped, "errors": errors, "results": results,
        }));
    } else {
        eprintln!();
        eprintln!("  {} built, {} cached, {} errors", built, skipped, errors);
    }

    if errors > 0 {
        WbExit::BlockFailed
    } else {
        WbExit::Success
    }
}

fn cmd_containers_list(json: bool) {
    let images = sandbox::list_images();
    if json {
        let arr: Vec<serde_json::Value> = images
            .iter()
            .map(|(tag, size, created)| {
                serde_json::json!({ "tag": tag, "size": size, "created": created })
            })
            .collect();
        print_json(&serde_json::json!({ "images": arr }));
        return;
    }
    if images.is_empty() {
        eprintln!("no sandbox images");
        return;
    }
    for (tag, size, created) in &images {
        println!("  {}  {}  {}", tag, size, created);
    }
}

fn cmd_containers_prune(json: bool) {
    let removed = sandbox::prune_images();
    if json {
        print_json(&serde_json::json!({ "removed": removed }));
    } else if removed == 0 {
        eprintln!("no sandbox images to remove");
    } else {
        eprintln!("removed {} sandbox images", removed);
    }
}

fn cmd_pending_impl(json_out: bool, no_reap: bool) {
    if !no_reap {
        let reaped = pending::reap_expired();
        if !reaped.is_empty() {
            eprintln!(
                "wb: reaped {} expired pending workbook(s) (on_timeout=abort):",
                reaped.len()
            );
            for r in &reaped {
                let kind = r.kind.as_deref().unwrap_or("-");
                let mode = r.on_timeout.as_deref().unwrap_or("abort");
                let ckpt_note = if r.checkpoint_marked_failed {
                    "checkpoint marked failed"
                } else {
                    "no checkpoint"
                };
                eprintln!(
                    "  {}  {}  kind={}  on_timeout={}  {}",
                    r.id, r.workbook, kind, mode, ckpt_note
                );
            }
        }
    }

    let descriptors = pending::list_all();

    if json_out {
        let entries: Vec<serde_json::Value> = descriptors
            .iter()
            .map(|(id, d)| {
                let mut val = serde_json::to_value(d).unwrap_or(serde_json::Value::Null);
                if let serde_json::Value::Object(ref mut map) = val {
                    map.insert("id".to_string(), serde_json::Value::String(id.clone()));
                }
                val
            })
            .collect();
        match serde_json::to_string_pretty(&entries) {
            Ok(s) => println!("{}", s),
            Err(e) => {
                eprintln!("error: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    if descriptors.is_empty() {
        eprintln!("no pending workbooks");
        return;
    }
    for (id, desc) in &descriptors {
        println!("{}", pending::summarize(id, desc));
    }
}

fn cmd_cancel(id: &str, json: bool) -> WbExit {
    let had_desc = pending::descriptor_path(id).exists();
    let had_ckpt = checkpoint::checkpoint_path(id).exists();
    if !had_desc && !had_ckpt {
        if json {
            print_json(&serde_json::json!({
                "ok": false, "id": id, "cancelled": false,
                "error": "no checkpoint or pending descriptor",
            }));
        }
        return WbExit::Io(format!("no checkpoint or pending descriptor for '{}'", id));
    }
    if let Err(e) = pending::delete(id) {
        log_warn!("warning: {}", e);
    }
    if let Err(e) = checkpoint::delete(id) {
        log_warn!("warning: {}", e);
    }
    if json {
        print_json(&serde_json::json!({ "ok": true, "id": id, "cancelled": true }));
    } else {
        eprintln!("cancelled '{}'", id);
    }
    WbExit::Success
}

fn cmd_resume_cmd(cli: ResumeArgs) -> WbExit {
    // Build env table: process env first, then --env-file overlays. Used to
    // resolve signal config when the user omits an id (auto-detect mode).
    // env-file paths are treated as cwd-relative here because the workbook
    // dir isn't known until after we resolve a checkpoint.
    let mut env_for_signal: std::collections::HashMap<String, String> = std::env::vars().collect();
    for path in &cli.env_files {
        match secrets::load_env_file(path) {
            Ok(env) => env_for_signal.extend(env),
            Err(e) => log_warn!("warning: env-file {}: {}", path, e),
        }
    }

    let signal_config = signal::config_from_env(&env_for_signal);

    // Resolve checkpoint id. If the user didn't pass one, scan pending
    // descriptors for a Redis-ready signal and use that. The signal is
    // consumed from Redis here; we thread it through as `preconsumed_signal`
    // so the per-id read below becomes a no-op for that case.
    let (id, preconsumed_signal): (String, Option<std::collections::HashMap<String, String>>) =
        match cli.id.clone() {
            Some(id) => (id, None),
            None => {
                let cfg = match signal_config.as_ref() {
                    Some(c) => c,
                    None => {
                        eprintln!(
                            "error: no checkpoint id provided and WB_SIGNAL_URL/WB_SIGNAL_KEY are not set."
                        );
                        eprintln!("hint: pass an id explicitly, or set WB_SIGNAL_URL + WB_SIGNAL_KEY to scan pending.");
                        std::process::exit(1);
                    }
                };
                match signal::find_ready_signal(cfg) {
                    Ok(Some((found_id, vars))) => {
                        eprintln!("wb: auto-detected pending '{}' with ready signal", found_id);
                        (found_id, Some(vars))
                    }
                    Ok(None) => {
                        eprintln!("wb: no pending workbooks have a ready signal");
                        std::process::exit(0);
                    }
                    Err(e) => {
                        eprintln!("error: scan pending: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        };
    let id = id.as_str();

    // Hold the same advisory lock `wb run` uses, so a concurrent run of the
    // same id fails fast instead of racing on checkpoint state. Released
    // explicitly before we re-enter `run_single`, which takes the lock itself.
    let resume_lock = match atomic_io::try_lock_for(&checkpoint::checkpoint_path(id)) {
        Ok(guard) => guard,
        Err(e) => {
            eprintln!(
                "error: checkpoint '{}' is in use by another process ({}). \
                 refusing to resume concurrently.",
                id, e
            );
            std::process::exit(exit_codes::EXIT_CHECKPOINT_BUSY);
        }
    };

    // Load the paused checkpoint.
    let mut ckpt = match checkpoint::load(id) {
        Ok(Some(c)) => c,
        Ok(None) => {
            eprintln!("error: no checkpoint '{}'", id);
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    if ckpt.status != checkpoint::CheckpointStatus::Paused
        && ckpt.status != checkpoint::CheckpointStatus::InProgress
        && ckpt.status != checkpoint::CheckpointStatus::Failed
    {
        eprintln!(
            "error: checkpoint '{}' is not paused (status: {:?})",
            id, ckpt.status
        );
        std::process::exit(exit_codes::EXIT_WORKBOOK_INVALID);
    }

    let desc = match pending::load(id) {
        Ok(Some(d)) => d,
        Ok(None) => {
            eprintln!("error: no pending descriptor for '{}'", id);
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    // Browser-slice pauses have an opaque sidecar_state instead of bind names.
    // Short-circuit the wait-flow validation, stash any signal payload for the
    // resumed run to pick up, and hand control straight to run_single.
    if desc.sidecar_state.is_some() {
        let signal_payload: Option<serde_json::Value> = if let Some(ref v) = cli.value {
            Some(serde_json::Value::String(v.clone()))
        } else if let Some(ref path) = cli.signal {
            let raw = if path == "-" {
                use std::io::Read;
                let mut buf = String::new();
                if let Err(e) = std::io::stdin().read_to_string(&mut buf) {
                    eprintln!("error: read stdin: {}", e);
                    std::process::exit(1);
                }
                buf
            } else {
                match std::fs::read_to_string(path) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("error: read {}: {}", path, e);
                        std::process::exit(1);
                    }
                }
            };
            match serde_json::from_str(&raw) {
                Ok(v) => Some(v),
                Err(e) => {
                    eprintln!("error: parse signal JSON: {}", e);
                    std::process::exit(1);
                }
            }
        } else if let Some(ref sig_vars) = preconsumed_signal {
            serde_json::to_value(sig_vars).ok()
        } else {
            None
        };

        // F7b: resolve the operator's navigation choice (CLI flag > signal
        // `action` object > plain forward resume).
        let action =
            resolve_resume_action(&cli.rerun_step, &cli.goto_step, signal_payload.as_ref());

        // For a rerun/goto we override the resume cursor and run the target
        // slice fresh from verb 0 (suppressing sidecar restore). A plain resume
        // keeps today's behavior.
        let mut browser_restart = false;
        let mut skipped_by_goto: std::collections::HashSet<usize> =
            std::collections::HashSet::new();
        if !matches!(action, ResumeAction::Resume) {
            let content = match std::fs::read_to_string(&ckpt.workbook) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("error: {}: {}", ckpt.workbook, e);
                    std::process::exit(1);
                }
            };
            let workbook = parse_and_resolve(&content, &ckpt.workbook);
            let steps = workbook.build_steps();
            let orig_paused = ckpt.next_block;
            let target_id = match &action {
                ResumeAction::RerunStep(None) => match steps.get(orig_paused) {
                    Some(s) => s.id.0.clone(),
                    None => {
                        eprintln!("error: cannot determine the paused step to re-run");
                        std::process::exit(exit_codes::EXIT_USAGE);
                    }
                },
                ResumeAction::RerunStep(Some(id)) | ResumeAction::GotoStep(id) => id.clone(),
                ResumeAction::Resume => unreachable!(),
            };
            let target_idx = match steps.iter().position(|s| s.id.as_str() == target_id) {
                Some(i) => i,
                None => {
                    eprintln!("error: step id '{}' not found in workbook", target_id);
                    std::process::exit(exit_codes::EXIT_USAGE);
                }
            };
            // Point the resume cursor at the target step.
            ckpt.next_block = target_idx;
            ckpt.next_step_id = steps.get(target_idx).map(|s| s.id.0.clone());
            if target_idx <= orig_paused {
                // Backward jump / rerun: drop stale results at or after the
                // target so the re-run doesn't double-count them.
                ckpt.results.retain(|r| r.block_index < target_idx);
            } else {
                // Forward jump: mark the stepped-over executable blocks so the
                // replay prefix emits step.skipped for them.
                skipped_by_goto = (orig_paused..target_idx).collect();
            }
            browser_restart = true;
            let verb = match &action {
                ResumeAction::GotoStep(_) => "goto_step",
                _ => "rerun_step",
            };
            eprintln!(
                "wb: {} → step '{}' (block {})",
                verb,
                target_id,
                target_idx + 1
            );
        }

        // Only a plain forward resume re-enters the paused slice via the
        // restore signal; a rerun/goto runs the target slice fresh.
        if !browser_restart {
            if let Some(ref sig) = signal_payload {
                if let Ok(serialized) = serde_json::to_string(sig) {
                    std::env::set_var("WB_BROWSER_RESUME_SIGNAL", serialized);
                }
            }
        }

        ckpt.mark_in_progress();
        if let Err(e) = checkpoint::save(id, &ckpt) {
            eprintln!("error: save checkpoint: {}", e);
            std::process::exit(1);
        }
        // Note: pending descriptor is left in place until run_single's browser
        // branch consumes sidecar_state; it's deleted once the slice completes.

        let workbook_file = ckpt.workbook.clone();
        let format_flag = if cli.json {
            Some(OutputFormat::Json)
        } else if cli.yaml {
            Some(OutputFormat::Yaml)
        } else if cli.md {
            Some(OutputFormat::Markdown)
        } else {
            None
        };
        let file_format = cli.output.as_deref().and_then(OutputFormat::from_path);
        let output_format = format_flag.or(file_format);
        let stdout_output = format_flag.is_some() && cli.output.is_none();
        let cli_vars: std::collections::HashMap<String, String> = cli
            .set_vars
            .iter()
            .filter_map(|s| {
                s.split_once('=')
                    .map(|(k, v)| (k.to_string(), v.to_string()))
            })
            .collect();

        run_single(RunConfig {
            file: workbook_file.clone(),
            output_path: cli.output,
            output_format,
            stdout_output,
            secrets_override: cli.secrets,
            project: cli.project,
            secrets_cmd: cli.secrets_cmd,
            dir: cli.dir,
            quiet: cli.quiet,
            bail: cli.bail,
            no_setup: cli.no_setup,
            checkpoint_id: Some(id.to_string()),
            callback_url: cli.callback,
            callback_secret: cli.callback_secret,
            callback_key: cli.callback_key,
            cli_vars,
            cli_redact: cli.redact,
            env_files: cli.env_files,
            env_file_relative: cli.env_file_relative,
            // Resume re-enters the original run's range; CLI selection
            // flags on the resume command would change semantics mid-run.
            selection: SelectionArgs::default(),
            default_block_timeout: None,
            browser_restart,
            skipped_by_goto,
            // Re-apply the original run's resolved params (resume has no
            // --param flags; a required param has no default to recover from).
            param_inputs: ckpt
                .params
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect(),
            param_file: None,
            profile: None,
            dry_run: false,
            cache_id: None,
            repair_url: None,
            repair_max: 0,
            events_path: None,
            sandbox: false,
            sandbox_no_network: false,
        });
        return WbExit::Success;
    }

    let expired = pending::is_expired(&desc);
    let on_timeout = desc.on_timeout.as_deref().unwrap_or("abort");

    // Determine payload values for bound names.
    let bind_names: Vec<String> = desc
        .bind
        .as_ref()
        .map(|b| match b {
            parser::BindSpec::Single(s) => vec![s.clone()],
            parser::BindSpec::Multiple(v) => v.clone(),
        })
        .unwrap_or_default();

    let mut new_vars: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    if expired && on_timeout == "abort" {
        eprintln!("error: wait expired and on_timeout=abort");
        ckpt.mark_failed();
        let _ = checkpoint::save(id, &ckpt);
        let _ = pending::delete(id);
        std::process::exit(exit_codes::EXIT_SIGNAL_TIMEOUT);
    }

    if expired && on_timeout == "skip" {
        for name in &bind_names {
            new_vars.insert(name.clone(), String::new());
        }
        eprintln!("wb: wait expired — binding empty values and skipping");
    } else if expired && on_timeout == "prompt" {
        use std::io::IsTerminal;
        if !std::io::stdin().is_terminal() {
            eprintln!(
                "error: wait expired and on_timeout=prompt, but no terminal is attached.\n\
                 Resolve by running `wb resume` interactively, or change on_timeout to abort/skip."
            );
            ckpt.mark_failed();
            let _ = checkpoint::save(id, &ckpt);
            let _ = pending::delete(id);
            std::process::exit(1);
        }
        let kind_str = desc.kind.as_deref().unwrap_or("-");
        let bind_str = desc
            .bind
            .as_ref()
            .map(|b| match b {
                parser::BindSpec::Single(s) => s.clone(),
                parser::BindSpec::Multiple(v) => v.join(", "),
            })
            .unwrap_or_else(|| "-".to_string());
        let timeout_str = desc.timeout_at.as_deref().unwrap_or("unknown");
        eprintln!("wb: wait expired");
        eprintln!("  kind:       {}", kind_str);
        eprintln!("  bind:       {}", bind_str);
        eprintln!("  timeout_at: {}", timeout_str);
        eprint!("  [r]etry / [s]kip / [a]bort? ");
        let mut answer = String::new();
        if let Err(e) = std::io::stdin().read_line(&mut answer) {
            eprintln!("error: read stdin: {}", e);
            std::process::exit(1);
        }
        match answer.trim().to_lowercase().as_str() {
            "r" | "retry" => {
                eprintln!(
                    "wb: retry selected — run `wb resume {}` again once the signal is ready.",
                    id
                );
                std::process::exit(0);
            }
            "s" | "skip" => {
                for name in &bind_names {
                    new_vars.insert(name.clone(), String::new());
                }
                eprintln!("wb: skipping — binding empty values");
            }
            _ => {
                // "a", "abort", or anything unrecognised defaults to abort.
                eprintln!("error: wait expired — aborting");
                ckpt.mark_failed();
                let _ = checkpoint::save(id, &ckpt);
                let _ = pending::delete(id);
                std::process::exit(1);
            }
        }
    } else if expired {
        // Unknown on_timeout value — default to abort.
        eprintln!(
            "error: wait expired and on_timeout='{}' is not recognised — aborting",
            on_timeout
        );
        ckpt.mark_failed();
        let _ = checkpoint::save(id, &ckpt);
        let _ = pending::delete(id);
        std::process::exit(exit_codes::EXIT_SIGNAL_TIMEOUT);
    } else {
        // Collect values from --value, --signal file/stdin, or --set.
        if let Some(ref v) = cli.value {
            if bind_names.len() == 1 {
                new_vars.insert(bind_names[0].clone(), v.clone());
            } else if bind_names.is_empty() {
                log_warn!("warning: --value provided but wait has no `bind` — ignoring");
            } else {
                eprintln!(
                    "error: --value only works for a single bind; this wait binds {:?}",
                    bind_names
                );
                std::process::exit(1);
            }
        }

        if let Some(ref path) = cli.signal {
            let raw = if path == "-" {
                use std::io::Read;
                let mut buf = String::new();
                if let Err(e) = std::io::stdin().read_to_string(&mut buf) {
                    eprintln!("error: read stdin: {}", e);
                    std::process::exit(1);
                }
                buf
            } else {
                match std::fs::read_to_string(path) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("error: read {}: {}", path, e);
                        std::process::exit(1);
                    }
                }
            };
            let val: serde_json::Value = match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("error: parse signal JSON: {}", e);
                    std::process::exit(1);
                }
            };
            merge_signal_into_vars(&val, &bind_names, &mut new_vars);
        }

        // --set key=val forwarding (optional extra vars).
        for pair in &cli.set_vars {
            if let Some((k, v)) = pair.split_once('=') {
                new_vars.insert(k.to_string(), v.to_string());
            }
        }

        // If no --value or --signal provided, try reading from Redis signal store.
        // (If we already consumed a signal during auto-detect, use that directly.)
        if new_vars.is_empty() {
            if let Some(sig_vars) = preconsumed_signal.as_ref() {
                let bound = signal::bind_signal_vars(sig_vars, &desc.bind);
                new_vars.extend(bound);
            } else if let Some(sig_config) = &signal_config {
                match signal::read_signal(sig_config, id) {
                    Ok(Some(sig_vars)) => {
                        let bound = signal::bind_signal_vars(&sig_vars, &desc.bind);
                        new_vars.extend(bound);
                        eprintln!("wb: signal read from {}", sig_config.signal_redis_key(id));
                    }
                    Ok(None) => {}
                    Err(e) => {
                        log_warn!("warning: {}", e);
                    }
                }
            }
        }

        // Verify required binds are satisfied.
        for name in &bind_names {
            if !new_vars.contains_key(name) {
                eprintln!(
                    "error: signal payload missing required bind '{}'. Provide via --signal <file> (JSON with key '{}'), --value <v>, or write to Redis signal key",
                    name, name
                );
                std::process::exit(1);
            }
        }
    }

    // Apply bindings and mark the wait satisfied.
    ckpt.bound_vars.extend(new_vars);
    ckpt.complete_wait(desc.section_index);
    ckpt.mark_in_progress();
    if let Err(e) = checkpoint::save(id, &ckpt) {
        eprintln!("error: save checkpoint: {}", e);
        std::process::exit(1);
    }
    let _ = pending::delete(id);

    // Re-enter the normal run flow using the original workbook path.
    let workbook_file = ckpt.workbook.clone();
    // Release the resume lock so run_single can take it; state is already
    // persisted, further writes happen via run_single's own lock acquisition.
    drop(resume_lock);

    let format_flag = if cli.json {
        Some(OutputFormat::Json)
    } else if cli.yaml {
        Some(OutputFormat::Yaml)
    } else if cli.md {
        Some(OutputFormat::Markdown)
    } else {
        None
    };
    let file_format = cli.output.as_deref().and_then(OutputFormat::from_path);
    let output_format = format_flag.or(file_format);
    let stdout_output = format_flag.is_some() && cli.output.is_none();

    let cli_vars: std::collections::HashMap<String, String> = cli
        .set_vars
        .iter()
        .filter_map(|s| {
            s.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect();

    run_single(RunConfig {
        file: workbook_file.clone(),
        output_path: cli.output,
        output_format,
        stdout_output,
        secrets_override: cli.secrets,
        project: cli.project,
        secrets_cmd: cli.secrets_cmd,
        dir: cli.dir,
        quiet: cli.quiet,
        bail: cli.bail,
        no_setup: cli.no_setup,
        checkpoint_id: Some(id.to_string()),
        callback_url: cli.callback,
        callback_secret: cli.callback_secret,
        callback_key: cli.callback_key,
        cli_vars,
        cli_redact: cli.redact,
        env_files: cli.env_files,
        env_file_relative: cli.env_file_relative,
        selection: SelectionArgs::default(),
        default_block_timeout: None,
        browser_restart: false,
        skipped_by_goto: std::collections::HashSet::new(),
        param_inputs: ckpt
            .params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect(),
        param_file: None,
        profile: None,
        dry_run: false,
        cache_id: None,
        repair_url: None,
        repair_max: 0,
        events_path: None,
        sandbox: false,
        sandbox_no_network: false,
    });
    WbExit::Success
}

/// Populate `out` with values from a signal JSON payload, keyed by bind names.
/// Rules (kept simple):
///  - If payload is an object, each bind name is pulled from its top-level key.
///    Also: any other top-level scalar keys are added as extra vars.
///  - If payload is a scalar and there's exactly one bind, it binds to that.
fn merge_signal_into_vars(
    val: &serde_json::Value,
    bind_names: &[String],
    out: &mut std::collections::HashMap<String, String>,
) {
    match val {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                if let Some(s) = json_scalar_to_string(v) {
                    out.insert(k.clone(), s);
                }
            }
            // Binds not satisfied by top-level keys will error in the caller.
            let _ = bind_names;
        }
        other => {
            if bind_names.len() == 1 {
                if let Some(s) = json_scalar_to_string(other) {
                    out.insert(bind_names[0].clone(), s);
                }
            }
        }
    }
}

fn json_scalar_to_string(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Null => Some(String::new()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn synthesized_sandbox_requires_picks_runtime() {
        let node_wb = parser::parse("---\nruntime: node\n---\n```js\nx\n```\n");
        assert_eq!(default_sandbox_requires(&node_wb).sandbox, "node");
        let bash_wb = parser::parse("---\nruntime: bash\n---\n```bash\nx\n```\n");
        // bash/unknown runtimes get a python base (has bash + a shell).
        assert_eq!(default_sandbox_requires(&bash_wb).sandbox, "python");
        let none_wb = parser::parse("```bash\nx\n```\n");
        assert_eq!(default_sandbox_requires(&none_wb).sandbox, "python");
    }

    #[test]
    fn remote_ref_resolution() {
        assert_eq!(
            resolve_remote_url("gh:rust-lang/rust/README.md"),
            Some("https://raw.githubusercontent.com/rust-lang/rust/HEAD/README.md".to_string())
        );
        assert_eq!(
            resolve_remote_url("gh:o/r/docs/x.md@v1.2"),
            Some("https://raw.githubusercontent.com/o/r/v1.2/docs/x.md".to_string())
        );
        assert_eq!(
            resolve_remote_url("https://example.com/x.md"),
            Some("https://example.com/x.md".to_string())
        );
        // Local paths and malformed gh refs are not remote.
        assert_eq!(resolve_remote_url("./local.md"), None);
        assert_eq!(resolve_remote_url("gh:onlyowner"), None);
    }

    // ─── F7b: resume navigation actions ──────────────────────────────

    #[test]
    fn resume_action_defaults_to_resume() {
        assert_eq!(
            resolve_resume_action(&None, &None, None),
            ResumeAction::Resume
        );
    }

    #[test]
    fn resume_action_cli_flags_win() {
        // --goto-step beats everything.
        assert_eq!(
            resolve_resume_action(&None, &Some("verify".into()), None),
            ResumeAction::GotoStep("verify".into())
        );
        // --rerun-step with no value = current step.
        assert_eq!(
            resolve_resume_action(&Some(None), &None, None),
            ResumeAction::RerunStep(None)
        );
        // --rerun-step <id>.
        assert_eq!(
            resolve_resume_action(&Some(Some("login".into())), &None, None),
            ResumeAction::RerunStep(Some("login".into()))
        );
    }

    #[test]
    fn resume_action_from_signal_object() {
        let sig = serde_json::json!({"action": {"kind": "goto_step", "target": "open-inbox"}});
        assert_eq!(
            resolve_resume_action(&None, &None, Some(&sig)),
            ResumeAction::GotoStep("open-inbox".into())
        );
        let rerun = serde_json::json!({"action": {"kind": "rerun_step"}});
        assert_eq!(
            resolve_resume_action(&None, &None, Some(&rerun)),
            ResumeAction::RerunStep(None)
        );
        // goto_step with no target is ignored (falls through to Resume).
        let bad = serde_json::json!({"action": {"kind": "goto_step"}});
        assert_eq!(
            resolve_resume_action(&None, &None, Some(&bad)),
            ResumeAction::Resume
        );
    }

    #[test]
    fn resume_action_cli_beats_signal() {
        let sig = serde_json::json!({"action": {"kind": "goto_step", "target": "from-signal"}});
        assert_eq!(
            resolve_resume_action(&Some(None), &None, Some(&sig)),
            ResumeAction::RerunStep(None)
        );
    }

    #[test]
    fn pause_action_targets_validated_against_steps() {
        let wb =
            parser::parse("```bash {#login}\necho hi\n```\n\n```bash {#verify}\necho ok\n```\n");
        let steps = wb.build_steps();
        let ok = vec![
            serde_json::json!({"kind": "rerun_step", "target": "verify"}),
            serde_json::json!({"kind": "goto_step", "target": "login"}),
            serde_json::json!({"kind": "rerun_step"}), // no target = current, valid
            serde_json::json!({"kind": "resume"}),     // non-nav, ignored
        ];
        assert!(validate_pause_action_targets(&ok, &steps).is_ok());

        let bad = vec![serde_json::json!({"kind": "goto_step", "target": "nope"})];
        assert_eq!(
            validate_pause_action_targets(&bad, &steps),
            Err("nope".to_string())
        );
    }

    // ─── merge_signal_into_vars ──────────────────────────────────────

    #[test]
    fn merge_signal_object_binds_matching_keys() {
        let val: serde_json::Value = serde_json::json!({"otp_code": "123456", "extra": "hello"});
        let bind_names = vec!["otp_code".to_string()];
        let mut out = HashMap::new();
        merge_signal_into_vars(&val, &bind_names, &mut out);
        assert_eq!(out.get("otp_code").unwrap(), "123456");
        assert_eq!(out.get("extra").unwrap(), "hello");
    }

    #[test]
    fn merge_signal_object_multi_bind() {
        let val: serde_json::Value = serde_json::json!({"code": "abc", "sender": "alice@test.com"});
        let bind_names = vec!["code".to_string(), "sender".to_string()];
        let mut out = HashMap::new();
        merge_signal_into_vars(&val, &bind_names, &mut out);
        assert_eq!(out.get("code").unwrap(), "abc");
        assert_eq!(out.get("sender").unwrap(), "alice@test.com");
    }

    #[test]
    fn merge_signal_object_missing_bind_key_not_inserted() {
        let val: serde_json::Value = serde_json::json!({"unrelated": "value"});
        let bind_names = vec!["otp_code".to_string()];
        let mut out = HashMap::new();
        merge_signal_into_vars(&val, &bind_names, &mut out);
        assert!(!out.contains_key("otp_code"));
        assert_eq!(out.get("unrelated").unwrap(), "value");
    }

    #[test]
    fn merge_signal_scalar_single_bind() {
        let val: serde_json::Value = serde_json::json!("plain-value");
        let bind_names = vec!["token".to_string()];
        let mut out = HashMap::new();
        merge_signal_into_vars(&val, &bind_names, &mut out);
        assert_eq!(out.get("token").unwrap(), "plain-value");
    }

    #[test]
    fn merge_signal_numeric_scalar() {
        let val: serde_json::Value = serde_json::json!(42);
        let bind_names = vec!["count".to_string()];
        let mut out = HashMap::new();
        merge_signal_into_vars(&val, &bind_names, &mut out);
        assert_eq!(out.get("count").unwrap(), "42");
    }

    #[test]
    fn merge_signal_bool_scalar() {
        let val: serde_json::Value = serde_json::json!(true);
        let bind_names = vec!["flag".to_string()];
        let mut out = HashMap::new();
        merge_signal_into_vars(&val, &bind_names, &mut out);
        assert_eq!(out.get("flag").unwrap(), "true");
    }

    #[test]
    fn merge_signal_null_scalar() {
        let val: serde_json::Value = serde_json::json!(null);
        let bind_names = vec!["maybe".to_string()];
        let mut out = HashMap::new();
        merge_signal_into_vars(&val, &bind_names, &mut out);
        assert_eq!(out.get("maybe").unwrap(), "");
    }

    #[test]
    fn merge_signal_scalar_ignored_when_multiple_binds() {
        let val: serde_json::Value = serde_json::json!("single");
        let bind_names = vec!["a".to_string(), "b".to_string()];
        let mut out = HashMap::new();
        merge_signal_into_vars(&val, &bind_names, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn merge_signal_object_skips_nested_values() {
        let val: serde_json::Value =
            serde_json::json!({"flat": "yes", "nested": {"deep": "value"}});
        let bind_names = vec!["flat".to_string()];
        let mut out = HashMap::new();
        merge_signal_into_vars(&val, &bind_names, &mut out);
        assert_eq!(out.get("flat").unwrap(), "yes");
        assert!(!out.contains_key("nested"));
    }

    #[test]
    fn merge_signal_empty_object_no_binds() {
        let val: serde_json::Value = serde_json::json!({});
        let bind_names: Vec<String> = vec![];
        let mut out = HashMap::new();
        merge_signal_into_vars(&val, &bind_names, &mut out);
        assert!(out.is_empty());
    }

    // ─── json_scalar_to_string ───────────────────────────────────────

    #[test]
    fn scalar_string() {
        let v = serde_json::json!("hello");
        assert_eq!(json_scalar_to_string(&v), Some("hello".to_string()));
    }

    #[test]
    fn scalar_number_int() {
        let v = serde_json::json!(99);
        assert_eq!(json_scalar_to_string(&v), Some("99".to_string()));
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn scalar_number_float() {
        let v = serde_json::json!(3.14);
        assert_eq!(json_scalar_to_string(&v), Some("3.14".to_string()));
    }

    #[test]
    fn scalar_bool_true() {
        assert_eq!(
            json_scalar_to_string(&serde_json::json!(true)),
            Some("true".to_string())
        );
    }

    #[test]
    fn scalar_bool_false() {
        assert_eq!(
            json_scalar_to_string(&serde_json::json!(false)),
            Some("false".to_string())
        );
    }

    #[test]
    fn scalar_null() {
        assert_eq!(
            json_scalar_to_string(&serde_json::json!(null)),
            Some(String::new())
        );
    }

    #[test]
    fn scalar_array_returns_none() {
        assert_eq!(json_scalar_to_string(&serde_json::json!([1, 2])), None);
    }

    #[test]
    fn scalar_object_returns_none() {
        assert_eq!(json_scalar_to_string(&serde_json::json!({"a": 1})), None);
    }

    // ─── checkpoint state transitions ────────────────────────────────

    #[test]
    fn checkpoint_new_is_in_progress() {
        let c = checkpoint::Checkpoint::new("test.md", 5);
        assert_eq!(c.status, checkpoint::CheckpointStatus::InProgress);
        assert_eq!(c.workbook, "test.md");
        assert_eq!(c.total_blocks, 5);
        assert_eq!(c.next_block, 0);
        assert!(c.bound_vars.is_empty());
        assert!(c.waits_completed.is_empty());
    }

    #[test]
    fn checkpoint_pause_and_resume_transitions() {
        let mut c = checkpoint::Checkpoint::new("test.md", 3);

        let result = executor::BlockResult {
            block_index: 0,
            language: "bash".to_string(),
            stdout: "ok".to_string(),
            stderr: String::new(),
            exit_code: 0,
            duration: std::time::Duration::from_millis(50),
            error_type: None,
            stdout_partial: false,
            stderr_partial: false,
        };
        c.add_result(&result, 10, Some("Setup"), "echo ok", None);
        assert_eq!(c.next_block, 1);
        assert_eq!(c.results.len(), 1);

        c.mark_paused();
        assert_eq!(c.status, checkpoint::CheckpointStatus::Paused);

        c.bound_vars
            .insert("otp_code".to_string(), "123456".to_string());
        c.complete_wait(2);
        assert!(c.waits_completed.contains(&2));

        c.mark_in_progress();
        assert_eq!(c.status, checkpoint::CheckpointStatus::InProgress);
        assert_eq!(c.bound_vars.get("otp_code").unwrap(), "123456");

        c.mark_complete();
        assert_eq!(c.status, checkpoint::CheckpointStatus::Complete);
    }

    #[test]
    fn checkpoint_complete_wait_is_idempotent() {
        let mut c = checkpoint::Checkpoint::new("test.md", 2);
        c.complete_wait(5);
        c.complete_wait(5);
        c.complete_wait(5);
        assert_eq!(c.waits_completed.len(), 1);
        assert_eq!(c.waits_completed[0], 5);
    }

    #[test]
    fn checkpoint_multiple_waits_tracked() {
        let mut c = checkpoint::Checkpoint::new("test.md", 4);
        c.complete_wait(2);
        c.complete_wait(7);
        c.complete_wait(12);
        assert_eq!(c.waits_completed.len(), 3);
        assert!(c.waits_completed.contains(&2));
        assert!(c.waits_completed.contains(&7));
        assert!(c.waits_completed.contains(&12));
    }

    #[test]
    fn checkpoint_bound_vars_merged_into_context() {
        let mut c = checkpoint::Checkpoint::new("test.md", 3);
        c.bound_vars
            .insert("api_key".to_string(), "secret123".to_string());
        c.bound_vars
            .insert("region".to_string(), "us-east-1".to_string());

        let mut vars: HashMap<String, String> = HashMap::new();
        vars.insert("existing".to_string(), "val".to_string());
        vars.extend(c.bound_vars.clone());

        assert_eq!(vars.get("existing").unwrap(), "val");
        assert_eq!(vars.get("api_key").unwrap(), "secret123");
        assert_eq!(vars.get("region").unwrap(), "us-east-1");
    }

    #[test]
    fn checkpoint_block_results_roundtrip() {
        let mut c = checkpoint::Checkpoint::new("test.md", 2);
        let r1 = executor::BlockResult {
            block_index: 0,
            language: "bash".to_string(),
            stdout: "hello\n".to_string(),
            stderr: String::new(),
            exit_code: 0,
            duration: std::time::Duration::from_millis(120),
            error_type: None,
            stdout_partial: false,
            stderr_partial: false,
        };
        let r2 = executor::BlockResult {
            block_index: 1,
            language: "python".to_string(),
            stdout: "42\n".to_string(),
            stderr: "warning\n".to_string(),
            exit_code: 0,
            duration: std::time::Duration::from_millis(300),
            error_type: None,
            stdout_partial: false,
            stderr_partial: false,
        };
        c.add_result(&r1, 5, None, "echo hello", None);
        c.add_result(&r2, 15, Some("Compute"), "print(42)", None);

        let restored = c.block_results();
        assert_eq!(restored.len(), 2);
        assert_eq!(restored[0].language, "bash");
        assert_eq!(restored[0].stdout, "hello\n");
        assert_eq!(restored[0].exit_code, 0);
        assert_eq!(restored[1].language, "python");
        assert_eq!(restored[1].stdout, "42\n");
        assert_eq!(restored[1].stderr, "warning\n");
    }

    #[test]
    fn checkpoint_code_hash_stored() {
        let mut c = checkpoint::Checkpoint::new("test.md", 1);
        let r = executor::BlockResult {
            block_index: 0,
            language: "bash".to_string(),
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
            duration: std::time::Duration::ZERO,
            error_type: None,
            stdout_partial: false,
            stderr_partial: false,
        };
        c.add_result(&r, 5, None, "echo hello", None);
        let hash = c.results[0].code_hash.as_ref().unwrap();
        assert_eq!(*hash, checkpoint::hash_code("echo hello"));
        assert_ne!(*hash, checkpoint::hash_code("echo goodbye"));
    }

    // ─── checkpoint save/load/delete ─────────────────────────────────

    #[test]
    fn checkpoint_save_load_delete_roundtrip() {
        let id = "test-save-load-roundtrip";
        let _ = checkpoint::delete(id);

        let mut c = checkpoint::Checkpoint::new("roundtrip.md", 3);
        c.mark_paused();
        c.bound_vars.insert("token".to_string(), "abc".to_string());
        c.complete_wait(4);

        checkpoint::save(id, &c).expect("save should succeed");

        let loaded = checkpoint::load(id)
            .expect("load should not error")
            .expect("checkpoint should exist");
        assert_eq!(loaded.status, checkpoint::CheckpointStatus::Paused);
        assert_eq!(loaded.workbook, "roundtrip.md");
        assert_eq!(loaded.total_blocks, 3);
        assert_eq!(loaded.bound_vars.get("token").unwrap(), "abc");
        assert!(loaded.waits_completed.contains(&4));

        checkpoint::delete(id).expect("delete should succeed");
        let gone = checkpoint::load(id).expect("load after delete should not error");
        assert!(gone.is_none());
    }

    // ─── pending descriptor build/save/load/delete ───────────────────

    #[test]
    fn pending_descriptor_roundtrip() {
        let id = "test-pending-roundtrip";
        let _ = pending::delete(id);

        let spec = parser::WaitSpec {
            kind: Some("email".to_string()),
            match_: None,
            bind: Some(parser::BindSpec::Single("otp".to_string())),
            timeout: Some("10m".to_string()),
            on_timeout: Some("abort".to_string()),
            line_number: 25,
            section_index: 3,
            attrs: Default::default(),
        };

        let desc = pending::build(id, "test-workbook.md", 2, None, &spec, None);
        assert_eq!(desc.checkpoint_id, id);
        assert_eq!(desc.workbook, "test-workbook.md");
        assert_eq!(desc.next_block, 2);
        assert_eq!(desc.line_number, 25);
        assert_eq!(desc.section_index, 3);
        assert_eq!(desc.kind.as_deref(), Some("email"));
        assert!(desc.timeout_at.is_some());
        assert_eq!(desc.on_timeout.as_deref(), Some("abort"));
        match &desc.bind {
            Some(parser::BindSpec::Single(s)) => assert_eq!(s, "otp"),
            _ => panic!("expected Single bind"),
        }

        pending::save(id, &desc).expect("save pending should succeed");

        let loaded = pending::load(id)
            .expect("load should not error")
            .expect("descriptor should exist");
        assert_eq!(loaded.checkpoint_id, id);
        assert_eq!(loaded.workbook, "test-workbook.md");
        assert_eq!(loaded.kind.as_deref(), Some("email"));
        assert!(!pending::is_expired(&loaded));

        pending::delete(id).expect("delete should succeed");
        let gone = pending::load(id).expect("load after delete should not error");
        assert!(gone.is_none());
    }

    #[test]
    fn pending_descriptor_no_timeout_never_expires() {
        let spec = parser::WaitSpec {
            kind: Some("manual".to_string()),
            match_: None,
            bind: None,
            timeout: None,
            on_timeout: None,
            line_number: 10,
            section_index: 1,
            attrs: Default::default(),
        };
        let desc = pending::build("no-timeout", "test.md", 0, None, &spec, None);
        assert!(desc.timeout_at.is_none());
        assert!(!pending::is_expired(&desc));
    }

    #[test]
    fn pending_descriptor_past_timeout_is_expired() {
        let spec = parser::WaitSpec {
            kind: Some("webhook".to_string()),
            match_: None,
            bind: Some(parser::BindSpec::Single("data".to_string())),
            timeout: Some("1s".to_string()),
            on_timeout: Some("skip".to_string()),
            line_number: 5,
            section_index: 2,
            attrs: Default::default(),
        };
        let mut desc = pending::build("expired-test", "test.md", 0, None, &spec, None);
        desc.timeout_at = Some("2020-01-01T00:00:00+00:00".to_string());
        assert!(pending::is_expired(&desc));
    }

    // ─── wait block parsing integration ──────────────────────────────

    #[test]
    fn parse_workbook_with_wait_gives_correct_section_indices() {
        let input = r#"# Test

```bash
echo "before"
```

```wait
kind: api
bind: result
timeout: 2m
```

```bash
echo "after $result"
```
"#;
        let wb = parser::parse(input);
        assert_eq!(wb.code_block_count(), 2);

        let mut wait_count = 0;
        let mut code_indices = Vec::new();
        for (i, section) in wb.sections.iter().enumerate() {
            match section {
                parser::Section::Wait(w) => {
                    wait_count += 1;
                    assert_eq!(w.kind.as_deref(), Some("api"));
                    assert_eq!(w.section_index, i);
                }
                parser::Section::Code(b) => {
                    code_indices.push((i, b.language.clone()));
                }
                _ => {}
            }
        }
        assert_eq!(wait_count, 1);
        assert_eq!(code_indices.len(), 2);
    }

    // ─── full wait/pause/resume subprocess integration ───────────────

    #[test]
    fn integration_wait_pause_resume_cycle() {
        let build = std::process::Command::new("cargo")
            .args(["build"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("cargo build should run");
        assert!(
            build.status.success(),
            "cargo build failed: {}",
            String::from_utf8_lossy(&build.stderr)
        );

        let wb_bin = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("debug")
            .join("wb");

        let tmp = std::env::temp_dir().join("wb-test-wait-cycle");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let workbook_path = tmp.join("wait-test.md");
        std::fs::write(
            &workbook_path,
            r#"# Wait Test

```bash
echo "step-1-before-wait"
```

```wait
kind: test
bind: my_var
timeout: 5m
```

```bash
echo "step-2-got: $my_var"
```
"#,
        )
        .unwrap();

        let ckpt_id = "integration-wait-test";
        let checkpoint_dir = checkpoint::checkpoint_dir();
        let _ = checkpoint::delete(ckpt_id);
        let _ = pending::delete(ckpt_id);

        // Run the workbook -- should pause at the wait block (exit 42)
        let run_output = std::process::Command::new(&wb_bin)
            .args([
                "run",
                workbook_path.to_str().unwrap(),
                "--checkpoint",
                ckpt_id,
            ])
            .env("WB_CHECKPOINT_DIR", &checkpoint_dir)
            .output()
            .expect("wb run should execute");

        let exit_code = run_output.status.code().unwrap_or(-1);
        assert_eq!(
            exit_code,
            42,
            "expected exit code 42 (paused), got {}.\nstderr: {}",
            exit_code,
            String::from_utf8_lossy(&run_output.stderr)
        );

        // Verify checkpoint is paused
        let ckpt = checkpoint::load(ckpt_id)
            .expect("load should not error")
            .expect("checkpoint should exist after pause");
        assert_eq!(ckpt.status, checkpoint::CheckpointStatus::Paused);
        assert_eq!(ckpt.next_block, 1);

        // Verify pending descriptor exists
        let desc = pending::load(ckpt_id)
            .expect("load pending should not error")
            .expect("pending descriptor should exist after pause");
        assert_eq!(desc.kind.as_deref(), Some("test"));
        match &desc.bind {
            Some(parser::BindSpec::Single(s)) => assert_eq!(s, "my_var"),
            _ => panic!("expected Single bind 'my_var'"),
        }

        // Create a signal payload and resume
        let signal_path = tmp.join("signal.json");
        std::fs::write(&signal_path, r#"{"my_var": "hello-from-signal"}"#).unwrap();

        let resume_output = std::process::Command::new(&wb_bin)
            .args(["resume", ckpt_id, "--signal", signal_path.to_str().unwrap()])
            .env("WB_CHECKPOINT_DIR", &checkpoint_dir)
            .output()
            .expect("wb resume should execute");

        let resume_exit = resume_output.status.code().unwrap_or(-1);
        assert_eq!(
            resume_exit,
            0,
            "expected exit code 0 after resume, got {}.\nstderr: {}\nstdout: {}",
            resume_exit,
            String::from_utf8_lossy(&resume_output.stderr),
            String::from_utf8_lossy(&resume_output.stdout),
        );

        // Verify bound var was available in resumed block
        let stdout = String::from_utf8_lossy(&resume_output.stdout);
        assert!(
            stdout.contains("step-2-got: hello-from-signal"),
            "expected bound var in output, got stdout: {}",
            stdout
        );

        // Verify checkpoint is complete
        let final_ckpt = checkpoint::load(ckpt_id)
            .expect("load should not error")
            .expect("checkpoint should still exist");
        assert_eq!(final_ckpt.status, checkpoint::CheckpointStatus::Complete);

        // Verify pending descriptor was cleaned up
        let pending_gone = pending::load(ckpt_id).expect("load should not error");
        assert!(
            pending_gone.is_none(),
            "pending descriptor should be deleted after resume"
        );

        // Clean up
        let _ = checkpoint::delete(ckpt_id);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn integration_resume_with_value_shorthand() {
        let build = std::process::Command::new("cargo")
            .args(["build"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("cargo build should run");
        assert!(build.status.success());

        let wb_bin = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("debug")
            .join("wb");

        let tmp = std::env::temp_dir().join("wb-test-value-shorthand");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let workbook_path = tmp.join("value-test.md");
        std::fs::write(
            &workbook_path,
            r#"```bash
echo "pre-wait"
```

```wait
kind: manual
bind: pin
timeout: 5m
```

```bash
echo "pin=$pin"
```
"#,
        )
        .unwrap();

        let ckpt_id = "integration-value-test";
        let checkpoint_dir = checkpoint::checkpoint_dir();
        let _ = checkpoint::delete(ckpt_id);
        let _ = pending::delete(ckpt_id);

        // Pause
        let run = std::process::Command::new(&wb_bin)
            .args([
                "run",
                workbook_path.to_str().unwrap(),
                "--checkpoint",
                ckpt_id,
            ])
            .env("WB_CHECKPOINT_DIR", &checkpoint_dir)
            .output()
            .expect("wb run");
        assert_eq!(run.status.code().unwrap_or(-1), 42);

        // Resume with --value instead of --signal
        let resume = std::process::Command::new(&wb_bin)
            .args(["resume", ckpt_id, "--value", "9999"])
            .env("WB_CHECKPOINT_DIR", &checkpoint_dir)
            .output()
            .expect("wb resume");

        assert_eq!(
            resume.status.code().unwrap_or(-1),
            0,
            "resume with --value failed: {}",
            String::from_utf8_lossy(&resume.stderr)
        );

        let stdout = String::from_utf8_lossy(&resume.stdout);
        assert!(
            stdout.contains("pin=9999"),
            "expected pin=9999 in output: {}",
            stdout
        );

        let _ = checkpoint::delete(ckpt_id);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ─── per-block policy: timeouts, retries, continue_on_error ──────

    fn quiet_bash_session() -> executor::Session {
        let ctx = executor::ExecutionContext {
            env: HashMap::new(),
            working_dir: ".".to_string(),
            venv: None,
            default_runtime: Some("bash".to_string()),
            exec_config: None,
            dir_config: None,
            quiet: true,
            vars: HashMap::new(),
            redact_values: Vec::new(),
            block_timeout: None,
        };
        executor::Session::new(ctx)
    }

    fn bash_block(code: &str) -> parser::CodeBlock {
        parser::CodeBlock {
            language: "bash".to_string(),
            code: code.to_string(),
            line_number: 0,
            skip_execution: false,
            silent: false,
            when: None,
            skip_if: None,
            no_cache: false,
            attrs: Default::default(),
        }
    }

    #[test]
    fn test_retry_runs_n_plus_1_attempts_on_persistent_failure() {
        let marker = std::env::temp_dir().join(format!(
            "wb-retry-persist-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&marker);
        let path_str = marker.to_string_lossy().to_string();

        let mut session = quiet_bash_session();
        // `false` sets $? to 1 without terminating the persistent bash
        // session (unlike `exit 1` in a `{ ... }` brace group, which would
        // kill the shell and break subsequent retries).
        let block = bash_block(&format!("echo x >> '{}'; false", path_str));
        let policy = parser::BlockPolicy {
            timeout_secs: None,
            retries: 2,
            continue_on_error: false,
        };
        let result = execute_block_with_policy(&mut session, &block, 0, policy, None, None, true);
        assert_eq!(result.exit_code, 1);
        let content = std::fs::read_to_string(&marker).expect("marker file should exist");
        assert_eq!(
            content.lines().count(),
            3,
            "expected 1 initial + 2 retries = 3 attempts; marker was: {:?}",
            content
        );
        let _ = std::fs::remove_file(&marker);
    }

    #[test]
    fn test_retry_stops_on_first_success() {
        // Uses a shell-level counter: the block "succeeds" on the second
        // attempt by bumping a file-backed counter. Proves the retry loop
        // breaks as soon as a run passes.
        let marker = std::env::temp_dir().join(format!(
            "wb-retry-success-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&marker);
        let path_str = marker.to_string_lossy().to_string();

        let mut session = quiet_bash_session();
        let code = format!(
            "echo x >> '{p}'; n=$(wc -l < '{p}' | tr -d ' '); if [ \"$n\" -ge 2 ]; then true; else false; fi",
            p = path_str
        );
        let block = bash_block(&code);
        let policy = parser::BlockPolicy {
            timeout_secs: None,
            retries: 5,
            continue_on_error: false,
        };
        let result = execute_block_with_policy(&mut session, &block, 0, policy, None, None, true);
        assert_eq!(result.exit_code, 0, "should succeed on 2nd attempt");
        let content = std::fs::read_to_string(&marker).expect("marker file should exist");
        assert_eq!(
            content.lines().count(),
            2,
            "should stop retrying once the attempt succeeded; marker was: {:?}",
            content
        );
        let _ = std::fs::remove_file(&marker);
    }

    #[test]
    fn test_per_block_timeout_override_triggers_partial() {
        // Short per-block timeout should fire and mark stdout_partial,
        // matching #10's behavior but via the policy path instead of a
        // hand-set ctx.block_timeout.
        let mut session = quiet_bash_session();
        let block = bash_block("echo before-sleep; sleep 5; echo after-sleep");
        let policy = parser::BlockPolicy {
            timeout_secs: Some(1),
            retries: 0,
            continue_on_error: false,
        };
        let result = execute_block_with_policy(&mut session, &block, 0, policy, None, None, true);
        assert!(
            result.stdout_partial,
            "timeout_secs=1 should trigger partial"
        );
        assert_eq!(result.error_type.as_deref(), Some("timeout"));
        assert!(
            result.stdout.contains("before-sleep"),
            "pre-timeout stdout should be preserved, got: {:?}",
            result.stdout
        );
    }

    #[test]
    fn test_default_timeout_caps_a_block_when_no_per_block_override() {
        // Pass a 1s default_timeout with no per-block override — the block
        // should be capped by the default and produce a timeout result.
        let mut session = quiet_bash_session();
        let block = bash_block("echo started; sleep 5; echo finished");
        let policy = parser::BlockPolicy {
            timeout_secs: None,
            retries: 0,
            continue_on_error: false,
        };
        let result = execute_block_with_policy(
            &mut session,
            &block,
            0,
            policy,
            Some(Duration::from_secs(1)),
            Some(DefaultTimeoutSource::FrontmatterDefault),
            true,
        );
        assert_eq!(result.error_type.as_deref(), Some("timeout"));
        assert!(result.stdout.contains("started"));
        assert!(!result.stdout.contains("finished"));
    }

    #[test]
    fn test_per_block_timeout_beats_default_timeout() {
        // Per-block override of 1s should win over a default of 30s. The
        // proof is that the block times out — if the default were applied,
        // the block (sleeping 5s) would still be running when the test
        // would expect a completed run, but the assertion that matters is
        // `error_type == timeout`.
        let mut session = quiet_bash_session();
        let block = bash_block("sleep 5");
        let policy = parser::BlockPolicy {
            timeout_secs: Some(1),
            retries: 0,
            continue_on_error: false,
        };
        let result = execute_block_with_policy(
            &mut session,
            &block,
            0,
            policy,
            Some(Duration::from_secs(30)),
            Some(DefaultTimeoutSource::FrontmatterDefault),
            true,
        );
        assert_eq!(result.error_type.as_deref(), Some("timeout"));
        // ~1s, not ~30s. Generous bound for slow CI.
        assert!(
            result.duration < Duration::from_secs(10),
            "should have timed out near 1s, not waited for the 30s default; got {:?}",
            result.duration
        );
    }

    #[test]
    fn test_unbounded_default_lets_block_run_past_short_window() {
        // No per-block override, no default — block must run to completion.
        let mut session = quiet_bash_session();
        let block = bash_block("sleep 1.5; echo done");
        let policy = parser::BlockPolicy {
            timeout_secs: None,
            retries: 0,
            continue_on_error: false,
        };
        let result = execute_block_with_policy(
            &mut session,
            &block,
            0,
            policy,
            None, // unbounded
            None,
            true,
        );
        assert_eq!(result.exit_code, 0, "should complete without a cap");
        assert!(result.error_type.is_none());
        assert!(result.stdout.contains("done"));
    }

    // --- write_pause_result (F1) ----------------------------------------------

    fn make_pause_tempdir() -> std::path::PathBuf {
        // Counter + pid + nanos beats pure time-based uniqueness: `cargo test`
        // runs tests on multiple threads and nano-resolution isn't enough on
        // some platforms (macOS in particular). A sibling test's cleanup
        // otherwise races our read and trips serde with "EOF while parsing".
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "wb-pause-test-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
            seq,
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_write_pause_result_value_shortcut() {
        // `wb resume X --value approved` arrives as `Some(String("approved"))`
        // at this helper. We wrap it as `{"value":"approved"}` so the file
        // shape is uniform regardless of how the operator resumed.
        let dir = make_pause_tempdir();
        let sig = serde_json::Value::String("approved".into());
        write_pause_result(&dir, Some(&sig));
        let content = std::fs::read_to_string(dir.join("pause_result.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["value"], "approved");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_write_pause_result_object_with_value_key() {
        // `wb resume X --signal payload.json` where payload = {"value": "X"}.
        // Unwrap the `value` key so downstream cells don't have to double-nest.
        let dir = make_pause_tempdir();
        let sig = serde_json::json!({"value": "denied", "note": "over budget"});
        write_pause_result(&dir, Some(&sig));
        let content = std::fs::read_to_string(dir.join("pause_result.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["value"], "denied");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_write_pause_result_object_without_value_key_preserves_whole_blob() {
        // Non-standard signal shape (no `value` key) — preserve it verbatim
        // under `.value` so authors who rely on custom signal payloads can
        // still read the whole object out.
        let dir = make_pause_tempdir();
        let sig = serde_json::json!({"approved_by": "alice", "amount": 420});
        write_pause_result(&dir, Some(&sig));
        let content = std::fs::read_to_string(dir.join("pause_result.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["value"]["approved_by"], "alice");
        assert_eq!(parsed["value"]["amount"], 420);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_write_pause_result_null_when_no_signal() {
        // Operator clicked Resume without passing --value/--signal. Still
        // write the file so the downstream cell's `test -f` check passes —
        // a default branch handles the null case.
        let dir = make_pause_tempdir();
        write_pause_result(&dir, None);
        let content = std::fs::read_to_string(dir.join("pause_result.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(parsed["value"].is_null());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ─── CLI arg parsing ──────────────────────────────────────────────

    #[test]
    fn parses_bare_run() {
        let cli = Cli::parse_from(["wb", "file.md"]);
        assert!(cli.command.is_none());
        assert_eq!(cli.bare_run.file.as_deref(), Some("file.md"));
    }

    #[test]
    fn parses_bare_run_with_json() {
        let cli = Cli::parse_from(["wb", "file.md", "--json"]);
        assert!(cli.command.is_none());
        assert_eq!(cli.bare_run.file.as_deref(), Some("file.md"));
        assert!(cli.bare_run.json);
    }

    #[test]
    fn parses_run_subcommand() {
        let cli = Cli::parse_from(["wb", "run", "file.md", "--bail"]);
        let Some(Command::Run(args)) = cli.command else {
            panic!("expected Run subcommand");
        };
        assert_eq!(args.file, "file.md");
        assert!(args.bail);
    }

    #[test]
    fn parses_inspect_subcommand() {
        let cli = Cli::parse_from(["wb", "inspect", "file.md"]);
        let Some(Command::Inspect(args)) = cli.command else {
            panic!("expected Inspect subcommand");
        };
        assert_eq!(args.file, "file.md");
        assert!(!args.json);

        let cli2 = Cli::parse_from(["wb", "inspect", "file.md", "--json"]);
        let Some(Command::Inspect(args2)) = cli2.command else {
            panic!("expected Inspect subcommand");
        };
        assert!(args2.json);
    }

    #[test]
    fn parses_containers_subcommands() {
        let cli = Cli::parse_from(["wb", "containers", "list"]);
        let Some(Command::Containers(args)) = cli.command else {
            panic!("expected Containers");
        };
        assert!(matches!(args.sub, ContainersSub::List { .. }));

        let cli2 = Cli::parse_from(["wb", "containers", "ls"]);
        let Some(Command::Containers(args2)) = cli2.command else {
            panic!("expected Containers");
        };
        assert!(matches!(args2.sub, ContainersSub::List { .. }));

        let cli3 = Cli::parse_from(["wb", "containers", "build", "some/dir"]);
        let Some(Command::Containers(args3)) = cli3.command else {
            panic!("expected Containers");
        };
        assert!(matches!(
            args3.sub,
            ContainersSub::Build { path: Some(_), .. }
        ));

        let cli4 = Cli::parse_from(["wb", "containers", "prune"]);
        let Some(Command::Containers(args4)) = cli4.command else {
            panic!("expected Containers");
        };
        assert!(matches!(args4.sub, ContainersSub::Prune { .. }));

        // --format json parses after the subcommand token.
        let cli5 = Cli::parse_from(["wb", "containers", "list", "--format", "json"]);
        let Some(Command::Containers(args5)) = cli5.command else {
            panic!("expected Containers");
        };
        assert!(matches!(
            args5.sub,
            ContainersSub::List { fmt } if fmt.format == "json"
        ));
    }

    #[test]
    fn parses_resume_with_value() {
        let cli = Cli::parse_from(["wb", "resume", "my-id", "--value", "12345"]);
        let Some(Command::Resume(args)) = cli.command else {
            panic!("expected Resume");
        };
        assert_eq!(args.id.as_deref(), Some("my-id"));
        assert_eq!(args.value.as_deref(), Some("12345"));
    }

    #[test]
    fn parses_pending_no_reap() {
        let cli = Cli::parse_from(["wb", "pending", "--no-reap"]);
        let Some(Command::Pending(args)) = cli.command else {
            panic!("expected Pending");
        };
        assert!(args.no_reap);
    }

    #[test]
    fn parses_generation_commands() {
        let cli = Cli::parse_from(["wb", "completion", "bash"]);
        let Some(Command::Completion { shell }) = cli.command else {
            panic!("expected Completion");
        };
        assert_eq!(shell, clap_complete::Shell::Bash);

        // zsh/fish parse too.
        let cli = Cli::parse_from(["wb", "completion", "zsh"]);
        assert!(matches!(
            cli.command,
            Some(Command::Completion {
                shell: clap_complete::Shell::Zsh
            })
        ));

        let cli = Cli::parse_from(["wb", "man"]);
        assert!(matches!(cli.command, Some(Command::Man)));
    }

    #[test]
    fn completion_generates_nonempty_script() {
        use clap::CommandFactory;
        let mut buf: Vec<u8> = Vec::new();
        let mut cmd = Cli::command();
        clap_complete::generate(clap_complete::Shell::Bash, &mut cmd, "wb", &mut buf);
        let script = String::from_utf8(buf).unwrap();
        assert!(script.contains("wb"), "completion script should mention wb");
        assert!(script.contains("validate"), "should list subcommands");
    }

    #[test]
    fn man_page_renders_nonempty() {
        use clap::CommandFactory;
        let man = clap_mangen::Man::new(Cli::command());
        let mut buf: Vec<u8> = Vec::new();
        man.render(&mut buf).unwrap();
        let page = String::from_utf8(buf).unwrap();
        assert!(page.contains("wb"), "man page should mention wb");
        assert!(page.contains(".TH"), "should be roff with a title header");
    }

    #[test]
    fn silent_skip_callbacks_emit_for_workflow_nodes_only() {
        let workflow = callback::WorkflowPayload {
            workflow: serde_json::json!({"slug": "wf"}),
            workflow_node: serde_json::json!({"id": "node"}),
        };
        assert!(should_emit_skip_callback(false, None));
        assert!(!should_emit_skip_callback(true, None));
        assert!(should_emit_skip_callback(true, Some(&workflow)));
    }

    #[test]
    fn wb_exit_codes_match_documented() {
        use exit::WbExit;
        assert_eq!(WbExit::Success.code(), exit_codes::EXIT_SUCCESS);
        assert_eq!(WbExit::BlockFailed.code(), exit_codes::EXIT_BLOCK_FAILED);
        assert_eq!(WbExit::Usage("x".into()).code(), exit_codes::EXIT_USAGE);
        assert_eq!(
            WbExit::WorkbookInvalid("x".into()).code(),
            exit_codes::EXIT_WORKBOOK_INVALID
        );
        assert_eq!(
            WbExit::SandboxUnavailable("x".into()).code(),
            exit_codes::EXIT_SANDBOX_UNAVAILABLE
        );
        assert_eq!(
            WbExit::CheckpointBusy("x".into()).code(),
            exit_codes::EXIT_CHECKPOINT_BUSY
        );
        assert_eq!(
            WbExit::SignalTimeout("x".into()).code(),
            exit_codes::EXIT_SIGNAL_TIMEOUT
        );
        assert_eq!(WbExit::Paused.code(), exit_codes::EXIT_PAUSED);
        assert_eq!(WbExit::Io("x".into()).code(), 1);
    }

    // ─── resume position resolution (Phase 2 of #29) ──────────────────

    fn fake_step(id: &str, language: &str) -> step_ir::Step {
        step_ir::Step {
            id: step_ir::StepId(id.to_string()),
            attrs: step_ir::FenceAttrs::default(),
            span: step_ir::Span::point(1),
            source: step_ir::Source {
                file: std::path::PathBuf::from("t.md"),
                position: 0,
            },
            language: language.to_string(),
            body: String::new(),
            include_chain: Vec::new(),
        }
    }

    #[test]
    fn resume_v1_legacy_matching_block_count_replays_from_next_block() {
        // No next_step_id (v1 checkpoint). total_blocks matches the current
        // workbook, so the numeric block_idx is safe to use.
        let mut c = checkpoint::Checkpoint::new("t.md", 3);
        c.next_block = 2;
        let steps = vec![
            fake_step("a", "bash"),
            fake_step("b", "bash"),
            fake_step("c", "bash"),
        ];
        match resolve_resume_position(&c, &steps, 3) {
            ResumeResolution::Replay { replay, notice } => {
                assert_eq!(replay, 2);
                assert!(notice.is_none(), "no notice for clean v1 resume");
            }
            ResumeResolution::Fresh(_) => panic!("expected Replay"),
        }
    }

    #[test]
    fn resume_v1_legacy_mismatched_block_count_starts_fresh() {
        let mut c = checkpoint::Checkpoint::new("t.md", 5);
        c.next_block = 2;
        let steps = vec![fake_step("a", "bash"), fake_step("b", "bash")];
        match resolve_resume_position(&c, &steps, 2) {
            ResumeResolution::Fresh(reason) => {
                assert!(reason.contains("5 → 2"), "reason: {}", reason);
            }
            ResumeResolution::Replay { .. } => panic!("expected Fresh"),
        }
    }

    #[test]
    fn resume_v2_step_id_at_same_position_no_notice() {
        let mut c = checkpoint::Checkpoint::new("t.md", 3);
        c.next_block = 1;
        c.next_step_id = Some("b".to_string());
        let steps = vec![
            fake_step("a", "bash"),
            fake_step("b", "bash"),
            fake_step("c", "bash"),
        ];
        match resolve_resume_position(&c, &steps, 3) {
            ResumeResolution::Replay { replay, notice } => {
                assert_eq!(replay, 1);
                assert!(notice.is_none(), "no notice when position is unchanged");
            }
            ResumeResolution::Fresh(_) => panic!("expected Replay"),
        }
    }

    #[test]
    fn resume_v2_step_id_shifted_picks_new_position() {
        // Saved checkpoint was at block 1 (id=b). A new block was inserted
        // above, shifting b to block 2 in the current workbook. Step-id-first
        // resume locates b at its new position and resumes there.
        let mut c = checkpoint::Checkpoint::new("t.md", 3);
        c.next_block = 1;
        c.next_step_id = Some("b".to_string());
        let steps = vec![
            fake_step("a", "bash"),
            fake_step("new-block", "bash"),
            fake_step("b", "bash"),
            fake_step("c", "bash"),
        ];
        match resolve_resume_position(&c, &steps, 4) {
            ResumeResolution::Replay { replay, notice } => {
                assert_eq!(replay, 2);
                let n = notice.expect("notice when block shifted");
                assert!(n.contains("'b' shifted"), "notice: {}", n);
                assert!(n.contains("block 2 to block 3"), "notice: {}", n);
            }
            ResumeResolution::Fresh(_) => panic!("expected Replay"),
        }
    }

    #[test]
    fn resume_v2_step_id_missing_falls_back_to_next_block_with_wb_resume_001() {
        // Saved id 'gone' no longer exists in the workbook (block deleted).
        // Fall back to the numeric next_block with a wb-resume-001 warning
        // since we can't prove the position is still correct.
        let mut c = checkpoint::Checkpoint::new("t.md", 3);
        c.next_block = 1;
        c.next_step_id = Some("gone".to_string());
        let steps = vec![
            fake_step("a", "bash"),
            fake_step("b", "bash"),
            fake_step("c", "bash"),
        ];
        match resolve_resume_position(&c, &steps, 3) {
            ResumeResolution::Replay { replay, notice } => {
                assert_eq!(replay, 1);
                let n = notice.expect("wb-resume-001 warning");
                assert!(n.contains("wb-resume-001"), "notice: {}", n);
                assert!(n.contains("'gone'"), "notice: {}", n);
            }
            ResumeResolution::Fresh(_) => panic!("expected Replay with warning"),
        }
    }

    #[test]
    fn resume_v2_step_id_missing_and_next_block_out_of_range_starts_fresh() {
        let mut c = checkpoint::Checkpoint::new("t.md", 5);
        c.next_block = 4;
        c.next_step_id = Some("gone".to_string());
        // Current workbook shrank below the saved next_block.
        let steps = vec![fake_step("a", "bash")];
        match resolve_resume_position(&c, &steps, 1) {
            ResumeResolution::Fresh(reason) => {
                assert!(reason.contains("'gone'"), "reason: {}", reason);
                assert!(reason.contains("out of range"), "reason: {}", reason);
            }
            ResumeResolution::Replay { .. } => panic!("expected Fresh"),
        }
    }

    // ─── selection flag resolution (Phase 4 of #29) ───────────────────

    #[test]
    fn selection_empty_returns_full_range() {
        let steps = vec![fake_step("a", "bash"), fake_step("b", "bash")];
        let sel = SelectionArgs::default();
        let r = resolve_selection(&sel, &steps, 2).expect("ok");
        assert_eq!(r, 0..2);
    }

    #[test]
    fn selection_only_returns_single_step_range() {
        let steps = vec![
            fake_step("a", "bash"),
            fake_step("b", "bash"),
            fake_step("c", "bash"),
        ];
        let sel = SelectionArgs {
            only: Some("b".to_string()),
            ..Default::default()
        };
        assert_eq!(resolve_selection(&sel, &steps, 3).expect("ok"), 1..2);
    }

    #[test]
    fn selection_from_skips_earlier_steps() {
        let steps = vec![
            fake_step("a", "bash"),
            fake_step("b", "bash"),
            fake_step("c", "bash"),
        ];
        let sel = SelectionArgs {
            from: Some("b".to_string()),
            ..Default::default()
        };
        assert_eq!(resolve_selection(&sel, &steps, 3).expect("ok"), 1..3);
    }

    #[test]
    fn selection_until_caps_later_steps() {
        let steps = vec![
            fake_step("a", "bash"),
            fake_step("b", "bash"),
            fake_step("c", "bash"),
        ];
        let sel = SelectionArgs {
            until: Some("b".to_string()),
            ..Default::default()
        };
        // --until is inclusive → range is exclusive-end after target.
        assert_eq!(resolve_selection(&sel, &steps, 3).expect("ok"), 0..2);
    }

    #[test]
    fn selection_from_and_until_form_an_explicit_range() {
        let steps = vec![
            fake_step("a", "bash"),
            fake_step("b", "bash"),
            fake_step("c", "bash"),
            fake_step("d", "bash"),
        ];
        let sel = SelectionArgs {
            from: Some("b".to_string()),
            until: Some("c".to_string()),
            ..Default::default()
        };
        assert_eq!(resolve_selection(&sel, &steps, 4).expect("ok"), 1..3);
    }

    #[test]
    fn selection_unknown_step_id_is_usage_error() {
        let steps = vec![fake_step("a", "bash")];
        let sel = SelectionArgs {
            only: Some("nope".to_string()),
            ..Default::default()
        };
        let err = resolve_selection(&sel, &steps, 1).expect_err("should reject unknown id");
        assert!(err.contains("'nope'"), "error: {}", err);
        assert!(err.contains("not found"), "error: {}", err);
    }

    #[test]
    fn selection_inverted_range_is_usage_error() {
        // --from c --until a: empty range. Reject so the user doesn't
        // silently run zero blocks.
        let steps = vec![
            fake_step("a", "bash"),
            fake_step("b", "bash"),
            fake_step("c", "bash"),
        ];
        let sel = SelectionArgs {
            from: Some("c".to_string()),
            until: Some("a".to_string()),
            ..Default::default()
        };
        let err = resolve_selection(&sel, &steps, 3).expect_err("empty range");
        assert!(err.contains("empty"), "error: {}", err);
    }

    #[test]
    fn selection_args_is_empty_only_true_when_all_none() {
        assert!(SelectionArgs::default().is_empty());
        assert!(!SelectionArgs {
            only: Some("x".into()),
            ..Default::default()
        }
        .is_empty());
        assert!(!SelectionArgs {
            from: Some("x".into()),
            ..Default::default()
        }
        .is_empty());
        assert!(!SelectionArgs {
            until: Some("x".into()),
            ..Default::default()
        }
        .is_empty());
    }

    #[test]
    fn clap_rejects_only_combined_with_from() {
        // Build a Cli that asks for both --only and --from. clap's
        // conflicts_with should reject it at parse time. (Cli doesn't
        // implement Debug, so we can't use expect_err — match on the Result.)
        use clap::Parser;
        match Cli::try_parse_from(["wb", "run", "f.md", "--only", "x", "--from", "y"]) {
            Ok(_) => panic!("--only + --from should conflict at parse time"),
            Err(e) => assert!(
                matches!(e.kind(), clap::error::ErrorKind::ArgumentConflict),
                "expected ArgumentConflict, got: {:?}",
                e.kind()
            ),
        }
    }

    #[test]
    fn clap_rejects_only_combined_with_until() {
        use clap::Parser;
        match Cli::try_parse_from(["wb", "run", "f.md", "--only", "x", "--until", "y"]) {
            Ok(_) => panic!("--only + --until should conflict at parse time"),
            Err(e) => assert!(
                matches!(e.kind(), clap::error::ErrorKind::ArgumentConflict),
                "expected ArgumentConflict, got: {:?}",
                e.kind()
            ),
        }
    }

    #[test]
    fn resume_v2_step_id_match_overrides_legacy_block_count_check() {
        // Critical Phase 2 behavior: when next_step_id is present, the
        // total_blocks invariant is *dropped*. A workbook that gained or lost
        // blocks (other than the resume target) can still be resumed.
        let mut c = checkpoint::Checkpoint::new("t.md", 3);
        c.next_block = 1;
        c.next_step_id = Some("b".to_string());
        // Current workbook has 5 blocks; saved had 3. Without step-id-first
        // resume the legacy code would have started fresh on count mismatch.
        let steps = vec![
            fake_step("a", "bash"),
            fake_step("inserted-1", "bash"),
            fake_step("b", "bash"),
            fake_step("inserted-2", "bash"),
            fake_step("c", "bash"),
        ];
        match resolve_resume_position(&c, &steps, 5) {
            ResumeResolution::Replay { replay, .. } => assert_eq!(replay, 2),
            ResumeResolution::Fresh(r) => {
                panic!("expected Replay across count change, got Fresh: {}", r)
            }
        }
    }
}
