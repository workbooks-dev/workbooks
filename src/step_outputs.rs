use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const ENV_OUTPUTS_PATH: &str = "WB_OUTPUTS_PATH";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputKind {
    String,
    Json,
}

impl OutputKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            OutputKind::String => "string",
            OutputKind::Json => "json",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapturedOutput {
    #[serde(rename = "type")]
    pub kind: OutputKind,
    pub value: Value,
}

pub type StepOutputMap = BTreeMap<String, CapturedOutput>;
pub type RawOutputsByStep = BTreeMap<String, BTreeMap<String, Value>>;

pub fn default_outputs_path(artifacts_dir: &Path) -> PathBuf {
    artifacts_dir.join(".wb").join("outputs.json")
}

pub fn init_outputs_path(
    env: &mut std::collections::HashMap<String, String>,
    artifacts_dir: &Path,
    outputs: &RawOutputsByStep,
) -> PathBuf {
    let path = default_outputs_path(artifacts_dir);
    env.insert(
        ENV_OUTPUTS_PATH.to_string(),
        path.to_string_lossy().into_owned(),
    );
    if let Err(e) = write_outputs_file(&path, outputs) {
        eprintln!("warning: outputs file: {}", e);
    }
    path
}

pub fn parse_outputs(stdout: &str) -> Result<StepOutputMap, String> {
    let mut outputs = StepOutputMap::new();
    for line in stdout.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("output-json:") {
            let (name, raw) = parse_assignment(rest, "output-json")?;
            let value: Value = serde_json::from_str(raw.trim())
                .map_err(|e| format!("output-json: {} has invalid JSON: {}", name, e))?;
            outputs.insert(
                name.to_string(),
                CapturedOutput {
                    kind: OutputKind::Json,
                    value,
                },
            );
        } else if let Some(rest) = trimmed.strip_prefix("output:") {
            let (name, value) = parse_assignment(rest, "output")?;
            outputs.insert(
                name.to_string(),
                CapturedOutput {
                    kind: OutputKind::String,
                    value: Value::String(value.to_string()),
                },
            );
        }
    }
    Ok(outputs)
}

pub fn is_output_capture_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("output:") || trimmed.starts_with("output-json:")
}

pub fn merge_step_outputs(all: &mut RawOutputsByStep, step_key: &str, captured: &StepOutputMap) {
    if captured.is_empty() {
        return;
    }
    let entry = all.entry(step_key.to_string()).or_default();
    for (name, output) in captured {
        entry.insert(name.clone(), output.value.clone());
    }
}

pub fn callback_outputs(outputs: &StepOutputMap) -> Value {
    let mut obj = serde_json::Map::new();
    for (name, output) in outputs {
        obj.insert(
            name.clone(),
            json!({
                "type": output.kind.as_str(),
                "value": output.value,
            }),
        );
    }
    Value::Object(obj)
}

pub fn write_outputs_file(path: &Path, outputs: &RawOutputsByStep) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {}", parent.display(), e))?;
    }
    let payload = json!({
        "steps": outputs.iter().map(|(step, values)| {
            (step.clone(), json!({ "outputs": values }))
        }).collect::<serde_json::Map<String, Value>>()
    });
    let bytes = serde_json::to_vec_pretty(&payload)
        .map_err(|e| format!("serialize {}: {}", path.display(), e))?;
    crate::atomic_io::write_secret_file(path, &bytes)
        .map_err(|e| format!("write {}: {}", path.display(), e))?;
    Ok(())
}

fn parse_assignment<'a>(rest: &'a str, prefix: &str) -> Result<(&'a str, &'a str), String> {
    let rest = rest.trim_start();
    let Some((name, value)) = rest.split_once('=') else {
        return Err(format!("{} line must be `{} name=value`", prefix, prefix));
    };
    let name = name.trim();
    if !valid_output_name(name) {
        return Err(format!(
            "{} name '{}' is invalid; expected [A-Za-z_][A-Za-z0-9_]*",
            prefix, name
        ));
    }
    Ok((name, value))
}

fn valid_output_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_string_and_json_outputs() {
        let out = parse_outputs(
            r#"hello
output: path=/tmp/report.csv
output-json: count=3
output-json: nested={"ok":true}
"#,
        )
        .unwrap();
        assert_eq!(out["path"].kind, OutputKind::String);
        assert_eq!(out["path"].value, Value::String("/tmp/report.csv".into()));
        assert_eq!(out["count"].value, json!(3));
        assert_eq!(out["nested"].value, json!({"ok": true}));
    }

    #[test]
    fn duplicate_outputs_last_wins() {
        let out = parse_outputs("output: name=first\noutput-json: name={\"v\":2}\n").unwrap();
        assert_eq!(out["name"].kind, OutputKind::Json);
        assert_eq!(out["name"].value, json!({"v": 2}));
    }

    #[test]
    fn rejects_invalid_names_and_json() {
        assert!(parse_outputs("output: 1bad=x").is_err());
        assert!(parse_outputs("output-json: ok={not json}").is_err());
    }

    #[test]
    fn detects_capture_lines() {
        assert!(is_output_capture_line("output: x=y"));
        assert!(is_output_capture_line("  output-json: x=1"));
        assert!(!is_output_capture_line("not output: x=y"));
    }

    #[test]
    fn aggregate_file_shape() {
        let mut all = RawOutputsByStep::new();
        let parsed = parse_outputs("output: path=/tmp/a\noutput-json: count=2\n").unwrap();
        merge_step_outputs(&mut all, "balance", &parsed);
        let tmp = std::env::temp_dir().join(format!(
            "wb-outputs-test-{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        write_outputs_file(&tmp, &all).unwrap();
        let v: Value = serde_json::from_slice(&fs::read(&tmp).unwrap()).unwrap();
        assert_eq!(v["steps"]["balance"]["outputs"]["path"], "/tmp/a");
        assert_eq!(v["steps"]["balance"]["outputs"]["count"], 2);
        let _ = fs::remove_file(tmp);
    }
}
