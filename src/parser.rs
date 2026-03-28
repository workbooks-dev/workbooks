use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Frontmatter {
    pub title: Option<String>,
    pub runtime: Option<String>,
    pub venv: Option<String>,
    pub shell: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub secrets: Option<SecretsConfig>,
    pub setup: Option<SetupConfig>,
    pub exec: Option<ExecConfig>,
    pub working_dir: Option<DirConfig>,
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
}

#[derive(Debug)]
pub enum Section {
    Text(String),
    Code(CodeBlock),
}

#[derive(Debug)]
pub struct Workbook {
    pub frontmatter: Frontmatter,
    pub sections: Vec<Section>,
    pub source: String,
}

impl Workbook {
    /// Count of executable code blocks
    pub fn code_block_count(&self) -> usize {
        self.sections
            .iter()
            .filter(|s| matches!(s, Section::Code(_)))
            .count()
    }
}

pub fn parse(input: &str) -> Workbook {
    let (frontmatter, body) = extract_frontmatter(input);
    let sections = extract_sections(&body);

    Workbook {
        frontmatter,
        sections,
        source: input.to_string(),
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

fn extract_sections(body: &str) -> Vec<Section> {
    let mut sections = Vec::new();
    let mut current_text = String::new();
    let mut lines = body.lines().enumerate().peekable();

    while let Some((line_num, line)) = lines.next() {
        if line.starts_with("```") && line.len() > 3 {
            // Opening fence with language
            let language = line[3..].trim().to_string();

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
}
