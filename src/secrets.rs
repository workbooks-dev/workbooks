use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::process::Command;

use crate::parser::{SecretProvider, SecretsConfig};

/// Resolve all secrets from configured providers into env vars
pub fn resolve_secrets(config: &SecretsConfig) -> Result<HashMap<String, String>, String> {
    let providers = match config {
        SecretsConfig::Single(p) => vec![p],
        SecretsConfig::Multiple(ps) => ps.iter().collect(),
    };

    let mut env = HashMap::new();
    for provider in providers {
        let resolved = resolve_provider(provider)?;
        env.extend(resolved);
    }
    Ok(env)
}

fn resolve_provider(provider: &SecretProvider) -> Result<HashMap<String, String>, String> {
    match provider.provider.as_str() {
        "env" => resolve_env(provider),
        "doppler" => resolve_doppler(provider),
        "yard" => resolve_yard(provider),
        "command" | "cmd" => resolve_command(provider),
        "prompt" => resolve_prompt(provider),
        "file" | "dotenv" => resolve_dotenv(provider),
        other => Err(format!("Unknown secret provider: {}", other)),
    }
}

/// Pull specific keys from the current environment
fn resolve_env(provider: &SecretProvider) -> Result<HashMap<String, String>, String> {
    let mut env = HashMap::new();
    if let Some(ref keys) = provider.keys {
        for key in keys {
            if let Ok(val) = std::env::var(key) {
                env.insert(key.clone(), val);
            }
        }
    }
    Ok(env)
}

/// Fetch secrets from Doppler CLI
fn resolve_doppler(provider: &SecretProvider) -> Result<HashMap<String, String>, String> {
    let mut cmd = Command::new("doppler");
    cmd.args(["secrets", "download", "--no-file", "--format", "json"]);
    if let Some(ref project) = provider.project {
        cmd.args(["--project", project]);
    }

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run doppler: {}. Is doppler CLI installed?", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("doppler failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let map: HashMap<String, serde_json::Value> = serde_json::from_str(&stdout)
        .map_err(|e| format!("Failed to parse doppler output: {}", e))?;

    Ok(map
        .into_iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
        .collect())
}

/// Fetch secrets from Yard CLI (yard env get)
fn resolve_yard(provider: &SecretProvider) -> Result<HashMap<String, String>, String> {
    let command = provider
        .command
        .as_deref()
        .unwrap_or("yard env get --format json");

    resolve_shell_command(command)
}

/// Run an arbitrary command that outputs KEY=VALUE lines or JSON
fn resolve_command(provider: &SecretProvider) -> Result<HashMap<String, String>, String> {
    let command = provider
        .command
        .as_deref()
        .ok_or("command provider requires a 'command' field")?;

    resolve_shell_command(command)
}

/// Interactively prompt for secret values
fn resolve_prompt(provider: &SecretProvider) -> Result<HashMap<String, String>, String> {
    let keys = provider
        .keys
        .as_ref()
        .ok_or("prompt provider requires 'keys' field listing which secrets to ask for")?;

    let mut env = HashMap::new();
    let stdin = io::stdin();
    let mut reader = stdin.lock();

    for key in keys {
        eprint!("Enter value for {}: ", key);
        io::stderr().flush().ok();
        let mut value = String::new();
        reader
            .read_line(&mut value)
            .map_err(|e| format!("Failed to read input: {}", e))?;
        env.insert(key.clone(), value.trim().to_string());
    }

    Ok(env)
}

/// Load secrets from a .env / dotenv file
fn resolve_dotenv(provider: &SecretProvider) -> Result<HashMap<String, String>, String> {
    let path = provider
        .command
        .as_deref()
        .unwrap_or(".env");

    let contents = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path, e))?;

    Ok(parse_env_lines(&contents))
}

/// Run a shell command and parse its output as env vars
fn resolve_shell_command(command: &str) -> Result<HashMap<String, String>, String> {
    let output = Command::new("sh")
        .args(["-c", command])
        .output()
        .map_err(|e| format!("Failed to run '{}': {}", command, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Command '{}' failed: {}", command, stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();

    // Try JSON first
    if trimmed.starts_with('{') {
        if let Ok(map) = serde_json::from_str::<HashMap<String, serde_json::Value>>(trimmed) {
            return Ok(map
                .into_iter()
                .filter_map(|(k, v)| {
                    let s = match v {
                        serde_json::Value::String(s) => s,
                        other => other.to_string(),
                    };
                    Some((k, s))
                })
                .collect());
        }
    }

    // Fall back to KEY=VALUE parsing
    Ok(parse_env_lines(trimmed))
}

fn parse_env_lines(input: &str) -> HashMap<String, String> {
    let mut env = HashMap::new();
    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            // Strip surrounding quotes
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(value);
            env.insert(key.to_string(), value.to_string());
        }
    }
    env
}
