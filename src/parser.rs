use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
}

#[derive(Debug)]
pub enum Section {
    Text(String),
    Code(CodeBlock),
    Wait(WaitSpec),
    Browser(BrowserSliceSpec),
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

/// Parsed info string: language token + optional `{no-run, silent, ...}` flags.
#[derive(Debug, Default, PartialEq, Eq)]
struct InfoString {
    language: String,
    skip_execution: bool,
    silent: bool,
}

/// Split a fence info string like `bash {no-run, silent}` into language + flags.
///
/// Brace cluster is optional and can appear anywhere after the language token. Flags
/// inside braces are comma or whitespace separated; unknown flags are currently
/// ignored so the parser stays forward-compatible.
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
            match flag {
                "no-run" => out.skip_execution = true,
                "silent" => out.silent = true,
                _ => {}
            }
        }
    }
    out
}

fn extract_sections(body: &str) -> Vec<Section> {
    // Info-string attribute flags (`{no-run}`, `{silent}`) are gated behind the
    // experimental env var. When disabled, fence lines like `bash {no-run}` stay
    // as-is and fall through to the non-executable-language branch — preserving
    // prior behavior so this feature is reversible.
    let flags_enabled = std::env::var("WB_EXPERIMENTAL_BLOCK_FLAGS").ok().as_deref() == Some("1");

    let mut sections = Vec::new();
    let mut current_text = String::new();
    let mut lines = body.lines().enumerate().peekable();

    while let Some((line_num, line)) = lines.next() {
        if line.starts_with("```") && line.len() > 3 {
            // Opening fence with language + optional `{flag, flag}` attribute cluster
            let info = if flags_enabled {
                parse_info_string(&line[3..])
            } else {
                InfoString {
                    language: line[3..].trim().to_string(),
                    ..Default::default()
                }
            };
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

    // --- extract_sections honors env gate ---
    //
    // These tests mutate WB_EXPERIMENTAL_BLOCK_FLAGS. A module-level mutex
    // serializes them so parallel test execution can't interleave env writes.

    use std::sync::Mutex;
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        prev: Option<String>,
    }

    impl EnvGuard {
        fn set(value: Option<&str>) -> Self {
            let prev = std::env::var("WB_EXPERIMENTAL_BLOCK_FLAGS").ok();
            match value {
                Some(v) => std::env::set_var("WB_EXPERIMENTAL_BLOCK_FLAGS", v),
                None => std::env::remove_var("WB_EXPERIMENTAL_BLOCK_FLAGS"),
            }
            EnvGuard { prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => std::env::set_var("WB_EXPERIMENTAL_BLOCK_FLAGS", v),
                None => std::env::remove_var("WB_EXPERIMENTAL_BLOCK_FLAGS"),
            }
        }
    }

    #[test]
    fn test_flags_gated_off_by_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::set(None);

        let input = r#"```bash {no-run}
echo "should not run"
```

```bash
echo "should run"
```
"#;
        let wb = parse(input);
        // With flag off: `bash {no-run}` is not recognized as executable
        // (the brace cluster isn't stripped), so it falls through to the
        // non-executable branch and is treated as documentation. Only the
        // plain `bash` block counts.
        assert_eq!(wb.code_block_count(), 1);
    }

    #[test]
    fn test_no_run_excluded_from_count() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::set(Some("1"));

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
        let _guard = ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::set(Some("1"));

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
        let _guard = ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::set(Some("1"));

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
}
