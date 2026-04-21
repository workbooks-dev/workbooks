mod artifacts;
mod atomic_io;
mod callback;
mod checkpoint;
mod exit_codes;
mod executor;
mod output;
mod parser;
mod pending;
mod sandbox;
mod secrets;
mod sidecar;
mod signal;
mod update;

use std::path::Path;
use std::time::{Duration, Instant};

/// Delay between retry attempts for per-block `retries:`. Short enough to
/// not feel sluggish, long enough to let a transient HTTP/API blip clear.
const RETRY_DELAY: Duration = Duration::from_millis(500);

/// Execute a code block once, applying the per-block timeout override if
/// set, and retrying on failure up to `policy.retries` times with a small
/// delay between attempts. Returns the result of the final attempt.
///
/// The session's block_timeout is always set before each attempt (to either
/// the per-block override or the run-wide default) so state can't leak
/// across blocks with different timeouts.
fn execute_block_with_policy(
    session: &mut executor::Session,
    block: &parser::CodeBlock,
    block_idx: usize,
    policy: parser::BlockPolicy,
    default_timeout: Duration,
    quiet: bool,
) -> executor::BlockResult {
    let timeout = policy
        .timeout_secs
        .map(Duration::from_secs)
        .unwrap_or(default_timeout);
    session.set_block_timeout(timeout);

    let mut attempt: u32 = 0;
    let total_attempts = 1 + policy.retries;
    loop {
        let result = session.execute_block(block, block_idx);
        attempt += 1;
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

use clap::Parser;
use output::OutputFormat;

#[derive(Parser)]
#[command(name = "wb", version, about = "Run markdown workbooks")]
struct Cli {
    /// Path to a markdown file or folder of workbooks
    file: Option<String>,

    /// Output file (format inferred from extension: .json, .yaml, .md)
    #[arg(short, long)]
    output: Option<String>,

    /// Output JSON (to stdout if no -o file)
    #[arg(long, group = "format")]
    json: bool,

    /// Output YAML (to stdout if no -o file)
    #[arg(long, group = "format")]
    yaml: bool,

    /// Output Markdown (to stdout if no -o file)
    #[arg(long, group = "format")]
    md: bool,

    /// Secret provider override (e.g., doppler, yard, env, prompt)
    #[arg(long)]
    secrets: Option<String>,

    /// Secret provider project (for doppler)
    #[arg(long)]
    project: Option<String>,

    /// Secret provider command (for yard/custom)
    #[arg(long = "secrets-cmd")]
    secrets_cmd: Option<String>,

    /// Working directory override
    #[arg(short = 'C', long)]
    dir: Option<String>,

    /// Suppress block output in terminal
    #[arg(short, long)]
    quiet: bool,

    /// Show block output in terminal (default; kept for backward compatibility)
    #[arg(short, long, hide = true)]
    verbose: bool,

    /// Stop on first failure
    #[arg(long)]
    bail: bool,

    /// Skip setup commands
    #[arg(long)]
    no_setup: bool,

    /// Inspect workbook structure without executing
    #[arg(short, long)]
    inspect: bool,

    /// Sort order for folder runs: a-z (default), z-a
    #[arg(long, default_value = "a-z")]
    order: String,

    /// Enable checkpointing with an ID (resumes if checkpoint exists)
    #[arg(long)]
    checkpoint: Option<String>,

    /// Callback URL to POST events to, or redis:// / rediss:// for Redis streams
    #[arg(long, env = "WB_CALLBACK_URL")]
    callback: Option<String>,

    /// HMAC-SHA256 secret for signing HTTP callback payloads (X-WB-Signature header)
    #[arg(long = "callback-secret", env = "WB_CALLBACK_SECRET")]
    callback_secret: Option<String>,

    /// Redis stream key for redis:// callbacks (default: wb:events)
    #[arg(long = "callback-key", env = "WB_CALLBACK_KEY")]
    callback_key: Option<String>,

    /// Set a variable (KEY=VALUE), overrides frontmatter vars
    #[arg(short = 'e', long = "set", value_name = "KEY=VALUE")]
    set_vars: Vec<String>,

    /// Load environment variables from a .env-style file (repeatable)
    #[arg(long = "env-file", value_name = "PATH")]
    env_files: Vec<String>,

    /// Resolve --env-file paths relative to the workbook file, not CWD
    #[arg(long = "env-file-relative")]
    env_file_relative: bool,

    /// Mark variable keys as secret (values redacted from output)
    #[arg(long)]
    redact: Vec<String>,
}

fn main() {
    // Intercept built-in commands before clap parses
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "update" => {
                let check_only = args.iter().any(|a| a == "--check");
                update::cmd_update(check_only);
                return;
            }
            "version" => {
                update::cmd_version();
                return;
            }
            "run" => {
                // Rewrite args: strip "run" so clap sees the file/folder as positional
                // wb run folder/ --order a-z  ->  wb folder/ --order a-z
                let mut new_args = vec![args[0].clone()];
                new_args.extend_from_slice(&args[2..]);
                let cli = Cli::parse_from(new_args);
                dispatch(cli);
                return;
            }
            "inspect" => {
                // wb inspect file.md  ->  wb file.md --inspect
                let mut new_args = vec![args[0].clone()];
                new_args.extend_from_slice(&args[2..]);
                new_args.push("--inspect".to_string());
                let cli = Cli::parse_from(new_args);
                dispatch(cli);
                return;
            }
            "transform" => {
                if args.len() < 3 {
                    eprintln!("usage: wb transform <file.md>");
                    std::process::exit(1);
                }
                transform_workbook(&args[2]);
                return;
            }
            "pending" => {
                cmd_pending(&args[2..]);
                return;
            }
            "cancel" => {
                if args.len() < 3 {
                    eprintln!("usage: wb cancel <checkpoint-id>");
                    std::process::exit(1);
                }
                cmd_cancel(&args[2]);
                return;
            }
            "resume" => {
                cmd_resume(&args[2..]);
                return;
            }
            "containers" => {
                cmd_containers(&args[2..]);
                return;
            }
            _ => {}
        }
    }

    let cli = Cli::parse();
    dispatch(cli);
}

fn dispatch(cli: Cli) {
    let path = match cli.file {
        Some(ref f) => f.as_str(),
        None => {
            eprintln!("usage: wb <file.md>");
            eprintln!("       wb run <folder/> -o report.json");
            eprintln!("       wb <file.md> --json");
            eprintln!("       wb update");
            std::process::exit(1);
        }
    };

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

    let p = Path::new(path);

    if p.is_dir() {
        run_folder(
            path,
            cli.output,
            output_format,
            stdout_output,
            cli.secrets,
            cli.project,
            cli.secrets_cmd,
            cli.dir,
            cli.quiet,
            cli.bail,
            &cli.order,
            cli.no_setup,
            cli_vars,
            cli.redact,
            cli.env_files,
            cli.env_file_relative,
        );
    } else if cli.inspect {
        if cli.json {
            inspect_workbook_json(path);
        } else {
            inspect_workbook(path);
        }
    } else {
        run_single(
            path,
            cli.output,
            output_format,
            stdout_output,
            cli.secrets.clone(),
            cli.project.clone(),
            cli.secrets_cmd.clone(),
            cli.dir,
            cli.quiet,
            cli.bail,
            cli.no_setup,
            cli.checkpoint,
            cli.callback,
            cli.callback_secret,
            cli.callback_key,
            cli_vars,
            cli.redact,
            cli.env_files,
            cli.env_file_relative,
        );
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

    let workbook = parser::parse(&content);
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

            let mut extra_args: Vec<String> = Vec::new();
            if quiet {
                extra_args.push("--quiet".to_string());
            }
            if no_setup {
                extra_args.push("--no-setup".to_string());
            }

            let start = Instant::now();
            let exit_code = match sandbox::run_in_sandbox(&tag, file, &container_env, &extra_args) {
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
                        stderr: e,
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

    let start = Instant::now();
    let mut results = Vec::new();
    let mut block_idx = 0;
    let mut session = executor::Session::new(ctx);

    for section in &workbook.sections {
        match section {
            parser::Section::Code(block) => {
                if block.skip_execution {
                    continue;
                }
                let policy = workbook
                    .frontmatter
                    .block_policy((block_idx + 1) as u32);
                let result = execute_block_with_policy(
                    &mut session,
                    block,
                    block_idx,
                    policy,
                    executor::DEFAULT_BLOCK_TIMEOUT,
                    quiet,
                );
                artifacts.sync();
                results.push(result);
                block_idx += 1;
            }
            parser::Section::Browser(spec) => {
                if spec.skip_execution {
                    continue;
                }
                let ctx = sidecar::SliceCallbackContext {
                    cb: None,
                    workbook: file,
                    checkpoint_id: None,
                    block_index: block_idx,
                    heading: None,
                    line_number: spec.line_number,
                    completed: block_idx + 1,
                    total: block_count,
                };
                let (result, pause) = session.execute_browser_slice(spec, block_idx, &ctx, None);
                artifacts.sync();
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
fn run_single(
    file: &str,
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
) {
    // Resolve the trace-correlation id once; flows into CallbackConfig and
    // the final RunSummary so every artifact/event of this run shares a key.
    let run_id = artifacts::resolve_run_id(&std::collections::HashMap::new());

    let content = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}: {}", file, e);
            std::process::exit(1);
        }
    };

    let workbook = parser::parse(&content);
    let block_count = workbook.code_block_count();

    if block_count == 0 {
        eprintln!(
            "no executable blocks in {}. Known runtimes: bash, sh, zsh, python, node, ruby, perl, r, php, lua, swift, go. \
             Check your fence language tags — `{{no-run}}` and `{{silent}}` are stable as of v0.9.8.",
            file
        );
        std::process::exit(exit_codes::EXIT_USAGE);
    }

    // Sandbox: if `requires` is present and we're not already inside a container,
    // build the image and re-invoke wb inside Docker. If Docker is missing the
    // build fails and we exit with EXIT_SANDBOX_UNAVAILABLE.
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

            // Forward CLI flags as extra args
            let mut extra_args: Vec<String> = Vec::new();
            if bail {
                extra_args.push("--bail".to_string());
            }
            if quiet {
                extra_args.push("--quiet".to_string());
            }
            if no_setup {
                extra_args.push("--no-setup".to_string());
            }
            if let Some(ref id) = checkpoint_id {
                extra_args.push("--checkpoint".to_string());
                extra_args.push(id.clone());
            }
            if let Some(ref url) = callback_url {
                extra_args.push("--callback".to_string());
                extra_args.push(url.clone());
            }
            if let Some(ref secret) = callback_secret {
                extra_args.push("--callback-secret".to_string());
                extra_args.push(secret.clone());
            }
            if let Some(ref key) = callback_key {
                extra_args.push("--callback-key".to_string());
                extra_args.push(key.clone());
            }
            if let Some(ref fmt_path) = output_path {
                extra_args.push("-o".to_string());
                extra_args.push(fmt_path.clone());
            }
            if let Some(ref fmt) = output_format {
                match fmt {
                    OutputFormat::Json => extra_args.push("--json".to_string()),
                    OutputFormat::Yaml => extra_args.push("--yaml".to_string()),
                    OutputFormat::Markdown => extra_args.push("--md".to_string()),
                }
            }

            let exit_code = match sandbox::run_in_sandbox(&tag, file, &container_env, &extra_args) {
                Ok(code) => code,
                Err(e) => {
                    eprintln!("error: sandbox run: {}", e);
                    1
                }
            };
            std::process::exit(exit_code);
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
        match secrets::resolve_secrets(config) {
            Ok(env) => ctx.env.extend(env),
            Err(e) => {
                eprintln!("error: secrets: {}", e);
                std::process::exit(1);
            }
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
                eprintln!("error: env-file: {}", e);
                std::process::exit(1);
            }
        }
    }

    // Merge vars: frontmatter defaults + CLI overrides
    let mut vars = workbook.frontmatter.vars.clone().unwrap_or_default();
    vars.extend(cli_vars);
    ctx.env.extend(vars.clone());
    ctx.vars = vars;

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
                eprintln!("error: {}", e);
                std::process::exit(1);
            }
        }
    }

    // Auto-generate checkpoint ID from workbook filename if not provided.
    let checkpoint_id = checkpoint_id.or_else(|| {
        Path::new(file)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
    });

    // Hold a session-long advisory lock on the checkpoint. Fails fast instead
    // of silently clobbering if another `wb run` or `wb resume` is live for
    // the same id. Dropped at end of function (including on panic).
    let _checkpoint_lock = match checkpoint_id.as_ref() {
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

    // Load checkpoint if resuming
    let (replay_until, mut results, mut ckpt) = if let Some(ref id) = checkpoint_id {
        match checkpoint::load(id) {
            Ok(Some(mut c))
                if c.status != checkpoint::CheckpointStatus::Complete
                    && c.workbook == file
                    && c.total_blocks == block_count =>
            {
                let replay = c.next_block;
                eprintln!(
                    "wb: resuming '{}' — replaying {} completed blocks to rebuild state",
                    id, replay
                );
                let prior = c.block_results();
                c.status = checkpoint::CheckpointStatus::InProgress;
                (replay, prior, Some(c))
            }
            Ok(_) => (
                0,
                Vec::new(),
                Some(checkpoint::Checkpoint::new(file, block_count)),
            ),
            Err(e) => {
                eprintln!("warning: {}", e);
                (
                    0,
                    Vec::new(),
                    Some(checkpoint::Checkpoint::new(file, block_count)),
                )
            }
        }
    } else {
        (0, Vec::new(), None)
    };

    // Merge signal-bound vars from the checkpoint into ctx before spawning sessions.
    // Second-line defense against a pre-validation-era checkpoint or a
    // hand-edited state file sneaking a reserved name past parse-time checks.
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

    // Resolve callback config: CLI/env flags take priority, then env-file values
    let resolved_callback_url = callback_url.or_else(|| ctx.env.get("WB_CALLBACK_URL").cloned());
    let resolved_callback_secret =
        callback_secret.or_else(|| ctx.env.get("WB_CALLBACK_SECRET").cloned());
    let resolved_callback_key = callback_key.or_else(|| ctx.env.get("WB_CALLBACK_KEY").cloned());

    let cb = resolved_callback_url.map(|url| callback::CallbackConfig {
        url,
        secret: resolved_callback_secret,
        stream_key: resolved_callback_key.unwrap_or_else(|| "wb:events".to_string()),
        run_id: run_id.clone(),
    });

    // Artifacts: same semantics as the non-checkpoint path — create/read the
    // dir, inject WB_ARTIFACTS_DIR, upload new files after each cell.
    let mut artifacts = artifacts::Artifacts::init(&mut ctx.env);

    let start = Instant::now();
    let mut block_idx = 0;
    let mut session = executor::Session::new(ctx);

    if !quiet {
        let title = workbook.frontmatter.title.as_deref().unwrap_or(file);
        eprintln!("{}", output::style_bold(title));
    }

    let mut last_heading: Option<String> = None;

    // Check for stale code hashes before replay
    if replay_until > 0 {
        if let Some(ref c) = ckpt {
            let mut stale_warned = false;
            let mut check_idx = 0;
            for section in &workbook.sections {
                if let parser::Section::Code(block) = section {
                    if check_idx >= replay_until {
                        break;
                    }
                    if let Some(saved) = c.results.get(check_idx) {
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
            if desc.sidecar_state.is_some() {
                browser_restore = Some(sidecar::RestoreArgs {
                    state: desc.sidecar_state.clone(),
                    signal: std::env::var("WB_BROWSER_RESUME_SIGNAL")
                        .ok()
                        .and_then(|s| serde_json::from_str(&s).ok()),
                });
            }
        }
    }

    for (section_idx, section) in workbook.sections.iter().enumerate() {
        if let parser::Section::Text(text) = section {
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("## ") {
                    last_heading = Some(trimmed.trim_start_matches('#').trim().to_string());
                }
            }
        }

        if let parser::Section::Wait(spec) = section {
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

            pause_for_signal(
                spec,
                section_idx,
                &checkpoint_id,
                ckpt.as_mut(),
                file,
                block_idx,
                block_count,
                start.elapsed(),
                &results,
                cb.as_ref(),
            );
            // Unreachable — pause_for_signal exits.
        }

        if let parser::Section::Code(block) = section {
            // `{no-run}`: parsed for docs tooling but never executes. Doesn't
            // advance block_idx (excluded from code_block_count), no callbacks,
            // not checkpointed, not in results. A one-line hint keeps the skip
            // visible in local runs.
            if block.skip_execution {
                if !quiet {
                    eprintln!(
                        "{}",
                        output::style_dim(&format!(
                            "  ⊘ skipped {{no-run}} [{}] (L{})",
                            block.language, block.line_number
                        ))
                    );
                }
                continue;
            }

            // Replay completed blocks to rebuild session state
            if block_idx < replay_until {
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
                let replay_result = session.execute_block(block, block_idx);
                session.set_quiet(quiet);

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

            if !quiet {
                let preview = block.code.lines().next();
                output::print_block_header(
                    block_heading.as_deref(),
                    &block.language,
                    block.line_number,
                    preview,
                );
            }

            let policy = workbook
                .frontmatter
                .block_policy((block_idx + 1) as u32);
            let result = execute_block_with_policy(
                &mut session,
                block,
                block_idx,
                policy,
                executor::DEFAULT_BLOCK_TIMEOUT,
                quiet,
            );
            artifacts.sync();
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
                    );
                }
            }

            if bail && !success && !policy.continue_on_error {
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
                    );
                }

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
                );
                if let Err(e) = checkpoint::save(ckpt_id, c) {
                    eprintln!("warning: checkpoint: {}", e);
                }
            }

            results.push(result);
            block_idx += 1;
        }

        if let parser::Section::Browser(spec) = section {
            // `{no-run}`: same semantics as code blocks — parsed but never
            // dispatched to the sidecar. The sidecar isn't spawned for a
            // run that has only no-run browser slices.
            if spec.skip_execution {
                if !quiet {
                    eprintln!(
                        "{}",
                        output::style_dim(&format!(
                            "  ⊘ skipped {{no-run}} [browser] (L{})",
                            spec.line_number
                        ))
                    );
                }
                continue;
            }

            // Replay path: browser sidecars rehydrate via persistent Browserbase
            // contexts, so a completed slice doesn't need to re-execute.
            if block_idx < replay_until {
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
            };
            // Take a one-shot restore, if the resume path handed us one earlier.
            let restore = browser_restore.take();
            let (result, pause_info) =
                session.execute_browser_slice(spec, block_idx, &slice_ctx, restore.as_ref());
            artifacts.sync();

            if let Some(pause) = pause_info {
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
                    );
                }

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
                );
                if let Err(e) = checkpoint::save(ckpt_id, c) {
                    eprintln!("warning: checkpoint: {}", e);
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

    if let Some(fmt) = output_format {
        let rendered = output::format_output(&workbook, &summary, fmt);

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

    if failed > 0 {
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

    let workbook = parser::parse(&content);

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
    let workbook = parser::parse(&content);

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
                blocks.push(serde_json::json!({
                    "index": idx,
                    "kind": "code",
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
                        v.iter().map(|s| serde_json::Value::String(s.clone())).collect(),
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
                blocks.push(serde_json::json!({
                    "index": idx,
                    "kind": "browser",
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

#[allow(clippy::too_many_arguments)]
fn pause_for_signal(
    spec: &parser::WaitSpec,
    section_idx: usize,
    checkpoint_id: &Option<String>,
    ckpt: Option<&mut checkpoint::Checkpoint>,
    file: &str,
    block_idx: usize,
    _block_count: usize,
    _elapsed: std::time::Duration,
    _results: &[executor::BlockResult],
    cb: Option<&callback::CallbackConfig>,
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
        c.mark_paused();
        if let Err(e) = checkpoint::save(id, c) {
            eprintln!("warning: checkpoint: {}", e);
        }
    }

    // Write pending-signal descriptor next to the checkpoint.
    let mut spec_with_idx = spec.clone();
    spec_with_idx.section_index = section_idx;
    let desc = pending::build(id, file, block_idx, &spec_with_idx);
    if let Err(e) = pending::save(id, &desc) {
        eprintln!("warning: pending descriptor: {}", e);
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
        );
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
    heading: Option<&str>,
    pause: sidecar::PauseInfo,
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

    if let Some(c) = ckpt {
        c.next_block = block_idx;
        c.mark_paused();
        if let Err(e) = checkpoint::save(id, c) {
            eprintln!("warning: checkpoint: {}", e);
        }
    }

    let desc = pending::build_for_browser_pause(
        id,
        file,
        block_idx,
        spec,
        pause.reason.clone(),
        pause.resume_url.clone(),
        pause.verb_index,
        pause.sidecar_state.clone(),
    );
    if let Err(e) = pending::save(id, &desc) {
        eprintln!("warning: pending descriptor: {}", e);
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
            None,
        );
        // Also fire step.paused with slice context for consumers that want
        // granular detail (verb index, live-view URL, heading, etc.).
        let mut extra = serde_json::Map::new();
        if let Some(reason) = pause.reason.as_ref() {
            extra.insert(
                "reason".to_string(),
                serde_json::Value::String(reason.clone()),
            );
        }
        if let Some(url) = pause.resume_url.as_ref() {
            extra.insert(
                "resume_url".to_string(),
                serde_json::Value::String(url.clone()),
            );
        }
        if let Some(vi) = pause.verb_index {
            extra.insert(
                "verb_index".to_string(),
                serde_json::Value::Number(vi.into()),
            );
        }
        cb.step_lifecycle(
            "step.paused",
            file,
            Some(id),
            block_idx,
            "browser",
            heading,
            spec.line_number,
            block_idx,
            0,
            serde_json::Value::Object(extra),
        );
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

    let workbook = parser::parse(&content);
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

fn cmd_containers(args: &[String]) {
    let sub = args.first().map(|s| s.as_str());
    match sub {
        Some("build") => cmd_containers_build(&args[1..]),
        Some("list") | Some("ls") => cmd_containers_list(),
        Some("prune") => cmd_containers_prune(),
        None | Some("help") | Some("--help") | Some("-h") => {
            print_containers_usage();
        }
        Some(other) => {
            eprintln!("error: unknown subcommand '{}'", other);
            eprintln!();
            print_containers_usage_stderr();
            std::process::exit(1);
        }
    }
}

fn print_containers_usage() {
    println!("usage: wb containers <build|list|prune>");
    println!();
    println!("Manage cached Docker images for sandboxed workbooks.");
    println!();
    println!("Subcommands:");
    println!("  build [path]   Build sandbox images for workbooks (file or directory)");
    println!("  list           List cached sandbox images");
    println!("  prune          Remove all sandbox images");
    println!();
    println!("Workbooks with a `requires:` frontmatter block build a Docker image on first run.");
    println!("Docker must be installed and running. If missing, `wb` exits with code 5.");
}

fn print_containers_usage_stderr() {
    eprintln!("usage: wb containers <build|list|prune>");
    eprintln!();
    eprintln!("  build [path]   Build sandbox images for workbooks (file or directory)");
    eprintln!("  list           List cached sandbox images");
    eprintln!("  prune          Remove all sandbox images");
}

fn cmd_containers_build(args: &[String]) {
    let path = args.first().map(|s| s.as_str()).unwrap_or(".");

    let files = if Path::new(path).is_dir() {
        collect_workbooks(path, "a-z")
    } else {
        vec![path.to_string()]
    };

    if files.is_empty() {
        eprintln!("no .md files found in {}", path);
        std::process::exit(0);
    }

    let mut built = 0;
    let mut skipped = 0;
    let mut errors = 0;

    for file in &files {
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("  error: {}: {}", file, e);
                errors += 1;
                continue;
            }
        };

        let workbook = parser::parse(&content);
        let requires = match workbook.frontmatter.requires {
            Some(ref r) => r,
            None => {
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
            eprintln!("  {} {} (cached: {})", "✓", filename, tag);
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
                eprintln!("  {} {} -> {}", "✓", filename, t);
                built += 1;
            }
            Err(e) => {
                eprintln!("  {} {} — {}", "✗", filename, e);
                errors += 1;
            }
        }
    }

    eprintln!();
    eprintln!("  {} built, {} cached, {} errors", built, skipped, errors);

    if errors > 0 {
        std::process::exit(1);
    }
}

fn cmd_containers_list() {
    let images = sandbox::list_images();
    if images.is_empty() {
        eprintln!("no sandbox images");
        return;
    }
    for (tag, size, created) in &images {
        println!("  {}  {}  {}", tag, size, created);
    }
}

fn cmd_containers_prune() {
    let removed = sandbox::prune_images();
    if removed == 0 {
        eprintln!("no sandbox images to remove");
    } else {
        eprintln!("removed {} sandbox images", removed);
    }
}

fn cmd_pending(args: &[String]) {
    let json_out = args.iter().any(|a| a == "--format=json" || a == "--json")
        || args
            .windows(2)
            .any(|w| w[0] == "--format" && w[1] == "json");

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

fn cmd_cancel(id: &str) {
    let had_desc = pending::descriptor_path(id).exists();
    let had_ckpt = checkpoint::checkpoint_path(id).exists();
    if !had_desc && !had_ckpt {
        eprintln!("no checkpoint or pending descriptor for '{}'", id);
        std::process::exit(1);
    }
    if let Err(e) = pending::delete(id) {
        eprintln!("warning: {}", e);
    }
    if let Err(e) = checkpoint::delete(id) {
        eprintln!("warning: {}", e);
    }
    eprintln!("cancelled '{}'", id);
}

#[derive(Parser)]
#[command(
    name = "wb resume",
    about = "Resume a paused workbook with a signal payload"
)]
struct ResumeCli {
    /// Checkpoint id to resume (omit to auto-detect from pending signals)
    id: Option<String>,

    /// Signal payload JSON file (use `-` for stdin)
    #[arg(long)]
    signal: Option<String>,

    /// Provide a single value for the bound var directly (shorthand for simple waits)
    #[arg(long)]
    value: Option<String>,

    /// Secret provider override
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

    /// Output file
    #[arg(short, long)]
    output: Option<String>,
    #[arg(long, group = "format")]
    json: bool,
    #[arg(long, group = "format")]
    yaml: bool,
    #[arg(long, group = "format")]
    md: bool,
}

fn cmd_resume(args: &[String]) {
    let mut parse_args = vec!["wb-resume".to_string()];
    parse_args.extend_from_slice(args);
    let cli = ResumeCli::parse_from(parse_args);

    // Build env table: process env first, then --env-file overlays. Used to
    // resolve signal config when the user omits an id (auto-detect mode).
    // env-file paths are treated as cwd-relative here because the workbook
    // dir isn't known until after we resolve a checkpoint.
    let mut env_for_signal: std::collections::HashMap<String, String> = std::env::vars().collect();
    for path in &cli.env_files {
        match secrets::load_env_file(path) {
            Ok(env) => env_for_signal.extend(env),
            Err(e) => eprintln!("warning: env-file {}: {}", path, e),
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

        if let Some(ref sig) = signal_payload {
            if let Ok(serialized) = serde_json::to_string(sig) {
                std::env::set_var("WB_BROWSER_RESUME_SIGNAL", serialized);
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

        run_single(
            &workbook_file,
            cli.output,
            output_format,
            stdout_output,
            cli.secrets,
            cli.project,
            cli.secrets_cmd,
            cli.dir,
            cli.quiet,
            cli.bail,
            cli.no_setup,
            Some(id.to_string()),
            cli.callback,
            cli.callback_secret,
            cli.callback_key,
            cli_vars,
            cli.redact,
            cli.env_files,
            cli.env_file_relative,
        );
        return;
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
                eprintln!("warning: --value provided but wait has no `bind` — ignoring");
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
                        eprintln!("warning: {}", e);
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

    run_single(
        &workbook_file,
        cli.output,
        output_format,
        stdout_output,
        cli.secrets,
        cli.project,
        cli.secrets_cmd,
        cli.dir,
        cli.quiet,
        cli.bail,
        cli.no_setup,
        Some(id.to_string()),
        cli.callback,
        cli.callback_secret,
        cli.callback_key,
        cli_vars,
        cli.redact,
        cli.env_files,
        cli.env_file_relative,
    );
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
        c.add_result(&result, 10, Some("Setup"), "echo ok");
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
        c.add_result(&r1, 5, None, "echo hello");
        c.add_result(&r2, 15, Some("Compute"), "print(42)");

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
        c.add_result(&r, 5, None, "echo hello");
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
        };

        let desc = pending::build(id, "test-workbook.md", 2, &spec);
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
        };
        let desc = pending::build("no-timeout", "test.md", 0, &spec);
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
        };
        let mut desc = pending::build("expired-test", "test.md", 0, &spec);
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
            .output()
            .expect("wb run");
        assert_eq!(run.status.code().unwrap_or(-1), 42);

        // Resume with --value instead of --signal
        let resume = std::process::Command::new(&wb_bin)
            .args(["resume", ckpt_id, "--value", "9999"])
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
            block_timeout: executor::DEFAULT_BLOCK_TIMEOUT,
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
        let result = execute_block_with_policy(
            &mut session,
            &block,
            0,
            policy,
            executor::DEFAULT_BLOCK_TIMEOUT,
            true,
        );
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
        let result = execute_block_with_policy(
            &mut session,
            &block,
            0,
            policy,
            executor::DEFAULT_BLOCK_TIMEOUT,
            true,
        );
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
        let result = execute_block_with_policy(
            &mut session,
            &block,
            0,
            policy,
            executor::DEFAULT_BLOCK_TIMEOUT,
            true,
        );
        assert!(result.stdout_partial, "timeout_secs=1 should trigger partial");
        assert_eq!(result.error_type.as_deref(), Some("timeout"));
        assert!(
            result.stdout.contains("before-sleep"),
            "pre-timeout stdout should be preserved, got: {:?}",
            result.stdout
        );
    }
}
