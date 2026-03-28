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

    /// Show block output in terminal
    #[arg(short, long)]
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
            cli.verbose,
            cli.bail,
            &cli.order,
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
            cli.verbose,
            cli.bail,
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
    verbose: bool,
    bail: bool,
    order: &str,
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
            verbose,
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
    verbose: bool,
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
                    code: String::new(),
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
    ctx.quiet = !verbose;

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
    verbose: bool,
    bail: bool,
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

    ctx.quiet = !verbose;

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

    let start = Instant::now();
    let mut results = Vec::new();
    let mut block_idx = 0;
    let mut session = executor::Session::new(ctx);

    for section in &workbook.sections {
        if let parser::Section::Code(block) = section {
            let result = session.execute_block(block, block_idx);
            let success = result.success();

            results.push(result);
            block_idx += 1;

            if bail && !success {
                break;
            }
        }
    }

    let total_duration = start.elapsed();
    let passed = results.iter().filter(|r| r.success()).count();
    let failed = results.iter().filter(|r| !r.success()).count();

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
