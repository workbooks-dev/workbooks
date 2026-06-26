use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{WbError, WbResult};

pub const ENV_OUTPUTS_PATH: &str = "WB_OUTPUTS_PATH";

/// Prefix under which captured step outputs are exported into the session env
/// so downstream `{when=...}` / `{skip_if=...}` (and child processes) can read a
/// value a prior step produced. Namespaced to avoid clobbering inherited
/// child-process env like `PATH` / `HOME`, since the session env is injected
/// into every bash/python/etc. block.
pub const OUTPUT_ENV_PREFIX: &str = "WB_OUT_";

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
        crate::log_warn!("warning: outputs file: {}", e);
    }
    path
}

pub fn parse_outputs(stdout: &str) -> WbResult<StepOutputMap> {
    let mut outputs = StepOutputMap::new();
    for line in stdout.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("output-json:") {
            let (name, raw) = parse_assignment(rest, "output-json")?;
            let value: Value = serde_json::from_str(raw.trim()).map_err(|e| {
                WbError::Parse(format!("output-json: {} has invalid JSON: {}", name, e))
            })?;
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

/// Compute the `(env_key, env_value)` pair a captured output maps to when
/// exported into the session env. String outputs use their raw string; JSON
/// outputs are stringified compactly so `{when=$WB_OUT_x}` can branch on them.
pub fn output_env_pair(name: &str, output: &CapturedOutput) -> (String, String) {
    let key = format!("{}{}", OUTPUT_ENV_PREFIX, name);
    let value = match (&output.kind, &output.value) {
        (OutputKind::String, Value::String(s)) => s.clone(),
        // Defensive: a String-kind output should always carry a JSON string,
        // but fall back to the compact rendering if it ever doesn't.
        (_, v) => v.to_string(),
    };
    (key, value)
}

/// Export captured outputs into the session env under [`OUTPUT_ENV_PREFIX`] so
/// later cells' `{when=...}` / `{skip_if=...}` and subsequent blocks can read
/// values a prior step produced. `{silent}` steps still export — env is
/// dataflow, not part of the notify stream.
pub fn export_to_session(session: &mut crate::executor::Session, captured: &StepOutputMap) {
    for (name, output) in captured {
        let (key, value) = output_env_pair(name, output);
        session.set_env(key, value);
    }
}

pub fn write_outputs_file(path: &Path, outputs: &RawOutputsByStep) -> WbResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| WbError::Io(format!("create {}: {}", parent.display(), e)))?;
    }
    let payload = json!({
        "steps": outputs.iter().map(|(step, values)| {
            (step.clone(), json!({ "outputs": values }))
        }).collect::<serde_json::Map<String, Value>>()
    });
    let bytes = serde_json::to_vec_pretty(&payload)
        .map_err(|e| WbError::Io(format!("serialize {}: {}", path.display(), e)))?;
    crate::atomic_io::write_secret_file(path, &bytes)
        .map_err(|e| WbError::Io(format!("write {}: {}", path.display(), e)))?;
    Ok(())
}

fn parse_assignment<'a>(rest: &'a str, prefix: &str) -> WbResult<(&'a str, &'a str)> {
    let rest = rest.trim_start();
    let Some((name, value)) = rest.split_once('=') else {
        return Err(WbError::Parse(format!(
            "{} line must be `{} name=value`",
            prefix, prefix
        )));
    };
    let name = name.trim();
    if !valid_output_name(name) {
        return Err(WbError::Parse(format!(
            "{} name '{}' is invalid; expected [A-Za-z_][A-Za-z0-9_]*",
            prefix, name
        )));
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
    fn output_env_pair_maps_string_and_json() {
        let parsed =
            parse_outputs("output: needs_login=1\noutput-json: count=3\noutput-json: flag=true\n")
                .unwrap();
        assert_eq!(
            output_env_pair("needs_login", &parsed["needs_login"]),
            ("WB_OUT_needs_login".to_string(), "1".to_string())
        );
        assert_eq!(
            output_env_pair("count", &parsed["count"]),
            ("WB_OUT_count".to_string(), "3".to_string())
        );
        assert_eq!(
            output_env_pair("flag", &parsed["flag"]),
            ("WB_OUT_flag".to_string(), "true".to_string())
        );
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

    #[test]
    fn output_kind_as_str_covers_both_variants() {
        // Line 29 (String) is exercised elsewhere; line 30 (Json) here.
        assert_eq!(OutputKind::String.as_str(), "string");
        assert_eq!(OutputKind::Json.as_str(), "json");
    }

    #[test]
    fn merge_step_outputs_noop_on_empty() {
        // Covers the early `return` (line 102).
        let mut all = RawOutputsByStep::new();
        let empty = StepOutputMap::new();
        merge_step_outputs(&mut all, "step", &empty);
        assert!(all.is_empty());
    }

    #[test]
    fn parse_assignment_missing_equals_errors() {
        // Covers the no-`=` branch in parse_assignment (lines 169-172).
        let err = parse_outputs("output: noequalshere\n").unwrap_err();
        assert!(format!("{err}").contains("name=value"), "got: {err}");
    }

    #[test]
    fn parse_assignment_empty_name_errors() {
        // `=value` yields an empty name -> valid_output_name's empty branch
        // (line 187) returns false, surfaced as an invalid-name error.
        let err = parse_outputs("output: =value\n").unwrap_err();
        assert!(format!("{err}").contains("invalid"), "got: {err}");
    }

    #[test]
    fn init_outputs_path_sets_env_and_writes() {
        let dir = tempfile::tempdir().unwrap();
        let mut env = std::collections::HashMap::new();
        let mut all = RawOutputsByStep::new();
        let parsed = parse_outputs("output: k=v\n").unwrap();
        merge_step_outputs(&mut all, "s", &parsed);
        let path = init_outputs_path(&mut env, dir.path(), &all);
        assert_eq!(env.get(ENV_OUTPUTS_PATH).map(String::as_str), path.to_str());
        assert!(path.exists());
    }

    #[test]
    fn init_outputs_path_warns_when_write_fails() {
        // Covers the write-error branch (line 60) and write_outputs_file's
        // create_dir_all error branch (line 153): a regular file blocks the
        // artifacts dir, so `<file>/.wb/outputs.json`'s parent can't be made.
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        fs::write(&blocker, b"i am a file").unwrap();
        let mut env = std::collections::HashMap::new();
        let all = RawOutputsByStep::new();
        // Returns the path (env still set) even though the write fails + warns.
        let path = init_outputs_path(&mut env, &blocker, &all);
        assert!(env.contains_key(ENV_OUTPUTS_PATH));
        assert!(!path.exists());
    }

    #[test]
    fn write_outputs_file_errors_under_a_file_path() {
        // Direct hit on write_outputs_file's create_dir_all error (line 153).
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        fs::write(&blocker, b"file").unwrap();
        let path = blocker.join("nested").join("outputs.json");
        let all = RawOutputsByStep::new();
        assert!(write_outputs_file(&path, &all).is_err());
    }
}
