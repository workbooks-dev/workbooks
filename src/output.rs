use std::io::IsTerminal;
use std::time::Duration;

use chrono::Utc;
use serde::Serialize;

use crate::executor::BlockResult;
use crate::parser::{Section, Workbook};

// ─── Terminal colors ──────────────────────────────────────────────────

fn use_color() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stderr().is_terminal()
}

pub fn style_ok(text: &str) -> String {
    if use_color() {
        format!("\x1b[32m{}\x1b[0m", text)
    } else {
        text.to_string()
    }
}

pub fn style_fail(text: &str) -> String {
    if use_color() {
        format!("\x1b[1;31m{}\x1b[0m", text)
    } else {
        text.to_string()
    }
}

pub fn style_dim(text: &str) -> String {
    if use_color() {
        format!("\x1b[2m{}\x1b[0m", text)
    } else {
        text.to_string()
    }
}

pub fn style_bold(text: &str) -> String {
    if use_color() {
        format!("\x1b[1m{}\x1b[0m", text)
    } else {
        text.to_string()
    }
}

/// Print a block's stderr line with dim styling
pub fn print_stderr_dim(line: &str) {
    if use_color() {
        eprintln!("\x1b[2m{}\x1b[0m", line);
    } else {
        eprintln!("{}", line);
    }
}

/// Print a dim separator rule before a block, with heading, language, and line number
pub fn print_block_header(heading: Option<&str>, language: &str, line_number: usize, code_preview: Option<&str>) {
    let label = match heading {
        Some(h) => format!("{} ({}, L{})", h, language, line_number),
        None => format!("{} (L{})", language, line_number),
    };
    let prefix = format!("── {} ", label);
    let pad = 60_usize.saturating_sub(prefix.chars().count());
    let rule = format!("{}{}", prefix, "─".repeat(pad));
    eprintln!("{}", style_dim(&rule));
    if let Some(preview) = code_preview {
        let truncated: String = preview.chars().take(72).collect();
        eprintln!("{}", style_dim(&format!("  {}", truncated)));
    }
}

pub struct RunSummary {
    pub source_file: String,
    pub total_blocks: usize,
    pub passed: usize,
    pub failed: usize,
    pub total_duration: Duration,
    pub results: Vec<BlockResult>,
    /// Trace-correlation id: `WB_RECORDING_RUN_ID` → `TRIGGER_RUN_ID` → generated.
    /// Stamped on result artifacts and callback payloads so a run is joinable
    /// across logs, dashboards, and artifact storage via a single key.
    pub run_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputFormat {
    Json,
    Yaml,
    Markdown,
}

impl OutputFormat {
    /// Infer format from file extension
    pub fn from_path(path: &str) -> Option<Self> {
        let ext = path.rsplit('.').next()?.to_lowercase();
        match ext.as_str() {
            "json" => Some(Self::Json),
            "yaml" | "yml" => Some(Self::Yaml),
            "md" | "markdown" => Some(Self::Markdown),
            _ => None,
        }
    }
}

#[derive(Serialize)]
struct JsonOutput {
    source: String,
    title: String,
    run_id: String,
    ran_at: String,
    duration_ms: u64,
    status: String,
    blocks: JsonBlocksSummary,
    results: Vec<JsonBlockResult>,
}

#[derive(Serialize)]
struct JsonBlocksSummary {
    total: usize,
    passed: usize,
    failed: usize,
}

#[derive(Serialize)]
struct JsonBlockResult {
    index: usize,
    language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    heading: Option<String>,
    line_number: usize,
    status: String,
    exit_code: i32,
    duration_ms: u64,
    #[serde(skip_serializing_if = "String::is_empty")]
    stdout: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    stderr: String,
    /// Machine-readable failure category; omitted on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    error_type: Option<String>,
    /// True when stdout was truncated by a block-level timeout before the
    /// runtime finished. Omitted from output when false so successful runs
    /// stay visually clean.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stdout_partial: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stderr_partial: bool,
}

/// Format results in the requested format
pub fn format_output(workbook: &Workbook, summary: &RunSummary, format: OutputFormat) -> String {
    match format {
        OutputFormat::Json => format_json(workbook, summary),
        OutputFormat::Yaml => format_yaml(workbook, summary),
        OutputFormat::Markdown => format_markdown(workbook, summary),
    }
}

fn build_json_output(workbook: &Workbook, summary: &RunSummary) -> JsonOutput {
    let title = workbook
        .frontmatter
        .title
        .as_deref()
        .unwrap_or(&summary.source_file)
        .to_string();

    // Build heading + line_number map from workbook sections
    let mut block_meta: Vec<(Option<String>, usize)> = Vec::new();
    let mut last_heading: Option<String> = None;
    for section in &workbook.sections {
        match section {
            Section::Text(text) => {
                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("## ") {
                        last_heading = Some(trimmed.trim_start_matches('#').trim().to_string());
                    }
                }
            }
            Section::Code(block) => {
                block_meta.push((last_heading.take(), block.line_number));
            }
            Section::Wait(_) => {}
            Section::Browser(_) => {}
            Section::Include(_) => {
                unreachable!(
                    "Section::Include must be resolved by parser::resolve_includes before output"
                )
            }
            Section::IncludeEnter(_) | Section::IncludeExit(_) => {}
        }
    }

    let results: Vec<JsonBlockResult> = summary
        .results
        .iter()
        .map(|r| {
            let (heading, line_number) = block_meta
                .get(r.block_index)
                .cloned()
                .unwrap_or((None, 0));
            JsonBlockResult {
                index: r.block_index,
                language: r.language.clone(),
                heading,
                line_number,
                status: if r.success() { "pass".into() } else { "fail".into() },
                exit_code: r.exit_code,
                duration_ms: r.duration.as_millis() as u64,
                stdout: r.stdout.clone(),
                stderr: r.stderr.clone(),
                error_type: r.error_type.clone(),
                stdout_partial: r.stdout_partial,
                stderr_partial: r.stderr_partial,
            }
        })
        .collect();

    JsonOutput {
        source: summary.source_file.clone(),
        title,
        run_id: summary.run_id.clone(),
        ran_at: Utc::now().to_rfc3339(),
        duration_ms: summary.total_duration.as_millis() as u64,
        status: if summary.failed == 0 { "pass".into() } else { "fail".into() },
        blocks: JsonBlocksSummary {
            total: summary.total_blocks,
            passed: summary.passed,
            failed: summary.failed,
        },
        results,
    }
}

fn format_json(workbook: &Workbook, summary: &RunSummary) -> String {
    let output = build_json_output(workbook, summary);
    serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
}

fn format_yaml(workbook: &Workbook, summary: &RunSummary) -> String {
    let output = build_json_output(workbook, summary);
    serde_yaml::to_string(&output).unwrap_or_else(|_| "---\n".to_string())
}

fn format_markdown(workbook: &Workbook, summary: &RunSummary) -> String {
    let mut out = String::new();

    let status = if summary.failed == 0 { "pass" } else { "fail" };
    let title = workbook
        .frontmatter
        .title
        .as_deref()
        .unwrap_or(&summary.source_file);

    out.push_str("---\n");
    out.push_str(&format!("source: {}\n", summary.source_file));
    out.push_str(&format!("title: {}\n", title));
    if !summary.run_id.is_empty() {
        out.push_str(&format!("run_id: {}\n", summary.run_id));
    }
    out.push_str(&format!("ran_at: {}\n", Utc::now().to_rfc3339()));
    out.push_str(&format!(
        "duration: {:.1}s\n",
        summary.total_duration.as_secs_f64()
    ));
    out.push_str(&format!("status: {}\n", status));
    out.push_str(&format!(
        "blocks: {{ total: {}, passed: {}, failed: {} }}\n",
        summary.total_blocks, summary.passed, summary.failed
    ));
    out.push_str("---\n\n");

    let mut result_idx = 0;
    for section in &workbook.sections {
        match section {
            Section::Text(text) => {
                out.push_str(text);
            }
            Section::Code(block) => {
                out.push_str(&format!("```{}\n", block.language));
                out.push_str(&block.code);
                out.push('\n');
                out.push_str("```\n\n");

                if result_idx < summary.results.len() {
                    let result = &summary.results[result_idx];
                    let status_marker = if result.success() { "pass" } else { "FAIL" };
                    out.push_str(&format!(
                        "**[{}]** block {} L{} _{:.1}s_\n",
                        status_marker,
                        result_idx + 1,
                        block.line_number,
                        result.duration.as_secs_f64()
                    ));

                    if !result.stdout.is_empty() {
                        out.push_str("```\n");
                        out.push_str(&result.stdout);
                        out.push('\n');
                        out.push_str("```\n");
                    }

                    if !result.stderr.is_empty() {
                        out.push_str("**stderr:**\n");
                        out.push_str("```\n");
                        out.push_str(&result.stderr);
                        out.push('\n');
                        out.push_str("```\n");
                    }

                    if !result.success() && result.stdout.is_empty() && result.stderr.is_empty() {
                        out.push_str(&format!("Exit code: {}\n", result.exit_code));
                    }

                    out.push('\n');
                    result_idx += 1;
                }
            }
            Section::Wait(spec) => {
                out.push_str("```wait\n");
                if let Ok(yaml) = serde_yaml::to_string(spec) {
                    out.push_str(&yaml);
                }
                out.push_str("```\n\n");
            }
            Section::Browser(spec) => {
                out.push_str("```browser\n");
                if !spec.raw.is_empty() {
                    out.push_str(&spec.raw);
                    if !spec.raw.ends_with('\n') {
                        out.push('\n');
                    }
                } else if let Ok(yaml) = serde_yaml::to_string(spec) {
                    out.push_str(&yaml);
                }
                out.push_str("```\n\n");
            }
            Section::Include(_) => {
                unreachable!(
                    "Section::Include must be resolved by parser::resolve_includes before output"
                )
            }
            Section::IncludeEnter(_) | Section::IncludeExit(_) => {}
        }
    }

    out.push_str("---\n\n");
    out.push_str(&format!(
        "_Ran {} blocks in {:.1}s — {} passed, {} failed_\n",
        summary.total_blocks,
        summary.total_duration.as_secs_f64(),
        summary.passed,
        summary.failed
    ));

    out
}

// --- Batch (folder) output ---

#[derive(Serialize)]
struct JsonBatchOutput {
    source: String,
    ran_at: String,
    duration_ms: u64,
    status: String,
    workbooks: JsonBatchWorkbooks,
    results: Vec<JsonBatchEntry>,
}

#[derive(Serialize)]
struct JsonBatchWorkbooks {
    total: usize,
    passed: usize,
    failed: usize,
}

#[derive(Serialize)]
struct JsonBatchEntry {
    file: String,
    status: String,
    blocks: JsonBlocksSummary,
    duration_ms: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    failures: Vec<JsonBlockResult>,
}

pub fn format_batch_output(
    summaries: &[RunSummary],
    dir: &str,
    total_duration: Duration,
    format: OutputFormat,
) -> String {
    match format {
        OutputFormat::Json => format_batch_json(summaries, dir, total_duration),
        OutputFormat::Yaml => format_batch_yaml(summaries, dir, total_duration),
        OutputFormat::Markdown => format_batch_markdown(summaries, dir, total_duration),
    }
}

fn build_batch_output(summaries: &[RunSummary], dir: &str, total_duration: Duration) -> JsonBatchOutput {
    let total = summaries.len();
    let failed_count = summaries.iter().filter(|s| s.failed > 0).count();

    let results: Vec<JsonBatchEntry> = summaries
        .iter()
        .map(|s| {
            let failures: Vec<JsonBlockResult> = s
                .results
                .iter()
                .filter(|r| !r.success())
                .map(|r| JsonBlockResult {
                    index: r.block_index,
                    language: r.language.clone(),
                    heading: None,
                    line_number: 0,
                    status: "fail".into(),
                    exit_code: r.exit_code,
                    duration_ms: r.duration.as_millis() as u64,
                    stdout: r.stdout.clone(),
                    stderr: r.stderr.clone(),
                    error_type: r.error_type.clone(),
                    stdout_partial: r.stdout_partial,
                    stderr_partial: r.stderr_partial,
                })
                .collect();

            JsonBatchEntry {
                file: s.source_file.clone(),
                status: if s.failed == 0 { "pass".into() } else { "fail".into() },
                blocks: JsonBlocksSummary {
                    total: s.total_blocks,
                    passed: s.passed,
                    failed: s.failed,
                },
                duration_ms: s.total_duration.as_millis() as u64,
                failures,
            }
        })
        .collect();

    JsonBatchOutput {
        source: dir.to_string(),
        ran_at: Utc::now().to_rfc3339(),
        duration_ms: total_duration.as_millis() as u64,
        status: if failed_count == 0 { "pass".into() } else { "fail".into() },
        workbooks: JsonBatchWorkbooks {
            total,
            passed: total - failed_count,
            failed: failed_count,
        },
        results,
    }
}

fn format_batch_json(summaries: &[RunSummary], dir: &str, total_duration: Duration) -> String {
    let output = build_batch_output(summaries, dir, total_duration);
    serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
}

fn format_batch_yaml(summaries: &[RunSummary], dir: &str, total_duration: Duration) -> String {
    let output = build_batch_output(summaries, dir, total_duration);
    serde_yaml::to_string(&output).unwrap_or_else(|_| "---\n".to_string())
}

fn format_batch_markdown(summaries: &[RunSummary], dir: &str, total_duration: Duration) -> String {
    let total = summaries.len();
    let failed_count = summaries.iter().filter(|s| s.failed > 0).count();
    let status = if failed_count == 0 { "pass" } else { "fail" };

    let mut out = String::new();

    out.push_str("---\n");
    out.push_str(&format!("source: {}\n", dir));
    out.push_str(&format!("ran_at: {}\n", Utc::now().to_rfc3339()));
    out.push_str(&format!("duration: {:.1}s\n", total_duration.as_secs_f64()));
    out.push_str(&format!("status: {}\n", status));
    out.push_str(&format!(
        "workbooks: {{ total: {}, passed: {}, failed: {} }}\n",
        total,
        total - failed_count,
        failed_count
    ));
    out.push_str("---\n\n");

    out.push_str(&format!("# Run Report: {}\n\n", dir));

    out.push_str("| Workbook | Status | Blocks | Time |\n");
    out.push_str("|----------|--------|--------|------|\n");

    for s in summaries {
        let name = std::path::Path::new(&s.source_file)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| s.source_file.clone());
        let status = if s.failed == 0 { "pass" } else { "FAIL" };
        out.push_str(&format!(
            "| {} | {} | {}/{} | {:.1}s |\n",
            name,
            status,
            s.passed,
            s.total_blocks,
            s.total_duration.as_secs_f64()
        ));
    }

    // Show failures detail
    let has_failures = summaries.iter().any(|s| s.failed > 0);
    if has_failures {
        out.push_str("\n## Failures\n\n");
        for s in summaries.iter().filter(|s| s.failed > 0) {
            out.push_str(&format!("### {}\n\n", s.source_file));
            for r in s.results.iter().filter(|r| !r.success()) {
                out.push_str(&format!("Block {} [{}] — exit {}\n", r.block_index + 1, r.language, r.exit_code));
                if !r.stderr.is_empty() {
                    out.push_str("```\n");
                    out.push_str(&r.stderr);
                    out.push('\n');
                    out.push_str("```\n");
                }
                out.push('\n');
            }
        }
    }

    out.push_str(&format!(
        "\n---\n\n_Ran {} workbooks in {:.1}s — {} passed, {} failed_\n",
        total,
        total_duration.as_secs_f64(),
        total - failed_count,
        failed_count
    ));

    out
}

/// One-line terminal summary to stderr
pub fn print_summary(summary: &RunSummary) {
    eprintln!();
    if summary.failed == 0 {
        eprintln!(
            "{}",
            style_ok(&format!(
                "✓ {} blocks in {:.1}s",
                summary.passed,
                summary.total_duration.as_secs_f64()
            ))
        );
    } else {
        eprintln!(
            "{}",
            style_fail(&format!(
                "✗ {} passed, {} failed in {:.1}s",
                summary.passed,
                summary.failed,
                summary.total_duration.as_secs_f64()
            ))
        );
    }
}
