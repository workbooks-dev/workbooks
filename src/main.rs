mod callback;
mod checkpoint;
mod executor;
mod output;
mod parser;
mod secrets;
mod update;

use std::path::Path;
use std::time::Instant;

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

    /// Callback URL to POST events to (step.complete, checkpoint.failed, run.complete)
    #[arg(long)]
    callback: Option<String>,

    /// HMAC-SHA256 secret for signing callback payloads (X-WB-Signature header)
    #[arg(long = "callback-secret")]
    callback_secret: Option<String>,

    /// Set a variable (KEY=VALUE), overrides frontmatter vars
    #[arg(short = 'e', long = "set", value_name = "KEY=VALUE")]
    set_vars: Vec<String>,

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
        .filter_map(|s| s.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
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
        );
    } else if cli.inspect {
        inspect_workbook(path);
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
            cli_vars,
            cli.redact,
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
        );

        let status = if summary.failed == 0 { "ok" } else { "FAIL" };
        eprintln!(
            "  {} {} ({}/{} blocks, {:.1}s)",
            status,
            filename,
            summary.passed,
            summary.total_blocks,
            summary.total_duration.as_secs_f64()
        );

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
            "ok — {} workbooks in {:.1}s",
            passed_workbooks,
            total_duration.as_secs_f64()
        );
    } else {
        eprintln!(
            "FAIL — {} passed, {} failed in {:.1}s",
            passed_workbooks,
            total_failed,
            total_duration.as_secs_f64()
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
        std::process::exit(1);
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
) -> output::RunSummary {
    let content = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(e) => {
            return output::RunSummary {
                source_file: file.to_string(),
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
                }],
            };
        }
    };

    let workbook = parser::parse(&content);
    let block_count = workbook.code_block_count();

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
                    }],
                };
            }
        }
    }

    let start = Instant::now();
    let mut results = Vec::new();
    let mut block_idx = 0;
    let mut session = executor::Session::new(ctx);

    for section in &workbook.sections {
        if let parser::Section::Code(block) = section {
            let result = session.execute_block(block, block_idx);
            results.push(result);
            block_idx += 1;
        }
    }

    let total_duration = start.elapsed();
    let passed = results.iter().filter(|r| r.success()).count();
    let failed = results.iter().filter(|r| !r.success()).count();

    output::RunSummary {
        source_file: file.to_string(),
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
    cli_vars: std::collections::HashMap<String, String>,
    cli_redact: Vec<String>,
) {
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
        eprintln!("no executable blocks in {}", file);
        std::process::exit(0);
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

    // Load checkpoint if resuming
    let (skip_until, mut results, mut ckpt) = if let Some(ref id) = checkpoint_id {
        match checkpoint::load(id) {
            Ok(Some(mut c))
                if c.status != checkpoint::CheckpointStatus::Complete
                    && c.workbook == file
                    && c.total_blocks == block_count =>
            {
                let skip = c.next_block;
                eprintln!(
                    "wb: resuming '{}' — skipping {} completed blocks",
                    id, skip
                );
                let prior = c.block_results();
                c.status = checkpoint::CheckpointStatus::InProgress;
                (skip, prior, Some(c))
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

    let cb = callback_url.map(|url| callback::CallbackConfig {
        url,
        secret: callback_secret,
    });

    let start = Instant::now();
    let mut block_idx = 0;
    let mut session = executor::Session::new(ctx);

    for section in &workbook.sections {
        if let parser::Section::Code(block) = section {
            if block_idx < skip_until {
                block_idx += 1;
                continue;
            }

            let result = session.execute_block(block, block_idx);
            let success = result.success();

            // Per-block progress
            eprintln!(
                "[{}/{}] {} {} ({:.1}s)",
                block_idx + 1,
                block_count,
                block.language,
                if success { "ok" } else { "FAIL" },
                result.duration.as_secs_f64()
            );

            // Callback: step complete (fires for every executed block)
            if let Some(ref cb) = cb {
                cb.step_complete(
                    &result,
                    block_idx + 1,
                    block_count,
                    file,
                    checkpoint_id.as_deref(),
                );
            }

            if bail && !success {
                // Callback: checkpoint failed (when checkpointing is active)
                if let (Some(ref cb), Some(ref ckpt_id)) = (&cb, &checkpoint_id) {
                    cb.checkpoint_failed(&result, block_idx, block_count, file, ckpt_id);
                }

                // Don't checkpoint the failed block — re-run it on resume
                if let Some(ref mut c) = ckpt {
                    c.mark_failed();
                    let _ = checkpoint::save(checkpoint_id.as_ref().unwrap(), c);
                }
                results.push(result);
                block_idx += 1;
                break;
            }

            // Checkpoint after each successful block (or any block without bail)
            if let Some(ref mut c) = ckpt {
                c.add_result(&result);
                if let Err(e) = checkpoint::save(checkpoint_id.as_ref().unwrap(), c) {
                    eprintln!("warning: checkpoint: {}", e);
                }
            }

            results.push(result);
            block_idx += 1;
        }
    }

    // Mark complete if all blocks ran
    if let Some(ref mut c) = ckpt {
        if c.status == checkpoint::CheckpointStatus::InProgress {
            c.mark_complete();
            let _ = checkpoint::save(checkpoint_id.as_ref().unwrap(), c);
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
        println!("vars: {}", vars.keys().cloned().collect::<Vec<_>>().join(", "));
    }
    if let Some(ref redact) = workbook.frontmatter.redact {
        println!("redact: {}", redact.join(", "));
    }
    if workbook.frontmatter.secrets.is_some() {
        println!("secrets: configured");
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
            println!("  {}. [{}] L{} -> {} — {}", idx, block.language, block.line_number, resolved, preview);
        }
    }

    if idx == 0 {
        println!("  (no executable blocks)");
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
