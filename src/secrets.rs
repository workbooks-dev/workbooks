use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::process::Command;

use crate::error::{WbError, WbResult};
use crate::parser::{SecretProvider, SecretsConfig};

/// Resolve all secrets from configured providers into env vars
pub fn resolve_secrets(config: &SecretsConfig) -> WbResult<HashMap<String, String>> {
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

fn resolve_provider(provider: &SecretProvider) -> WbResult<HashMap<String, String>> {
    match provider.provider.as_str() {
        "env" => resolve_env(provider),
        "doppler" => resolve_doppler(provider),
        "yard" => resolve_yard(provider),
        "command" | "cmd" => resolve_command(provider),
        "prompt" => resolve_prompt(provider),
        "file" | "dotenv" => resolve_dotenv(provider),
        other => Err(WbError::Secret(format!(
            "Unknown secret provider: {}",
            other
        ))),
    }
}

/// Pull specific keys from the current environment
fn resolve_env(provider: &SecretProvider) -> WbResult<HashMap<String, String>> {
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
fn resolve_doppler(provider: &SecretProvider) -> WbResult<HashMap<String, String>> {
    let mut cmd = Command::new("doppler");
    cmd.args(["secrets", "download", "--no-file", "--format", "json"]);
    if let Some(ref project) = provider.project {
        cmd.args(["--project", project]);
    }

    let output = cmd.output().map_err(|e| {
        WbError::Secret(format!(
            "Failed to run doppler: {}. Is doppler CLI installed?",
            e
        ))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WbError::Secret(format!("doppler failed: {}", stderr)));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let map: HashMap<String, serde_json::Value> = serde_json::from_str(&stdout)
        .map_err(|e| WbError::Secret(format!("Failed to parse doppler output: {}", e)))?;

    Ok(map
        .into_iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
        .collect())
}

/// Fetch secrets from Yard CLI (yard env get)
fn resolve_yard(provider: &SecretProvider) -> WbResult<HashMap<String, String>> {
    let command = provider
        .command
        .as_deref()
        .unwrap_or("yard env get --format json");

    resolve_shell_command(command)
}

/// Run an arbitrary command that outputs KEY=VALUE lines or JSON
fn resolve_command(provider: &SecretProvider) -> WbResult<HashMap<String, String>> {
    let command = provider
        .command
        .as_deref()
        .ok_or_else(|| WbError::Secret("command provider requires a 'command' field".into()))?;

    resolve_shell_command(command)
}

/// Interactively prompt for secret values
fn resolve_prompt(provider: &SecretProvider) -> WbResult<HashMap<String, String>> {
    let keys = provider.keys.as_ref().ok_or_else(|| {
        WbError::Secret(
            "prompt provider requires 'keys' field listing which secrets to ask for".into(),
        )
    })?;

    let mut env = HashMap::new();
    let stdin = io::stdin();
    let mut reader = stdin.lock();

    for key in keys {
        eprint!("Enter value for {}: ", key);
        io::stderr().flush().ok();
        let mut value = String::new();
        reader
            .read_line(&mut value)
            .map_err(|e| WbError::Secret(format!("Failed to read input: {}", e)))?;
        env.insert(key.clone(), value.trim().to_string());
    }

    Ok(env)
}

/// Load secrets from a .env / dotenv file
fn resolve_dotenv(provider: &SecretProvider) -> WbResult<HashMap<String, String>> {
    let path = provider.command.as_deref().unwrap_or(".env");

    load_env_file(path)
}

/// Read a .env-style file from disk and parse it into a map of env vars.
pub fn load_env_file(path: &str) -> WbResult<HashMap<String, String>> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| WbError::Secret(format!("Failed to read {}: {}", path, e)))?;
    Ok(parse_env_lines(&contents))
}

/// Run a shell command and parse its output as env vars
fn resolve_shell_command(command: &str) -> WbResult<HashMap<String, String>> {
    let output = Command::new("sh")
        .args(["-c", command])
        .output()
        .map_err(|e| WbError::Secret(format!("Failed to run '{}': {}", command, e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WbError::Secret(format!(
            "Command '{}' failed: {}",
            command, stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();

    // Try JSON first
    if trimmed.starts_with('{') {
        if let Ok(map) = serde_json::from_str::<HashMap<String, serde_json::Value>>(trimmed) {
            return Ok(map
                .into_iter()
                .map(|(k, v)| {
                    let s = match v {
                        serde_json::Value::String(s) => s,
                        other => other.to_string(),
                    };
                    (k, s)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(name: &str) -> SecretProvider {
        SecretProvider {
            provider: name.to_string(),
            project: None,
            command: None,
            keys: None,
        }
    }

    #[test]
    fn parse_env_lines_handles_quotes_comments_and_blanks() {
        let input = "# a comment\n\nFOO=bar\nQUOTED=\"hello world\"\nSINGLE='single quoted'\nNAKED= spaced \nNOEQUALS\n  # indented comment\n";
        let env = parse_env_lines(input);
        assert_eq!(env.get("FOO").unwrap(), "bar");
        assert_eq!(env.get("QUOTED").unwrap(), "hello world");
        assert_eq!(env.get("SINGLE").unwrap(), "single quoted");
        assert_eq!(env.get("NAKED").unwrap(), "spaced");
        assert!(!env.contains_key("NOEQUALS"));
        // comment lines are skipped
        assert_eq!(env.len(), 4);
    }

    #[test]
    fn resolve_env_pulls_present_keys_only() {
        std::env::set_var("WB_TEST_SECRET_PRESENT", "yes");
        std::env::remove_var("WB_TEST_SECRET_ABSENT");
        let mut p = provider("env");
        p.keys = Some(vec![
            "WB_TEST_SECRET_PRESENT".to_string(),
            "WB_TEST_SECRET_ABSENT".to_string(),
        ]);
        let env = resolve_env(&p).unwrap();
        assert_eq!(env.get("WB_TEST_SECRET_PRESENT").unwrap(), "yes");
        assert!(!env.contains_key("WB_TEST_SECRET_ABSENT"));
        std::env::remove_var("WB_TEST_SECRET_PRESENT");
    }

    #[test]
    fn resolve_env_with_no_keys_is_empty() {
        let p = provider("env");
        let env = resolve_env(&p).unwrap();
        assert!(env.is_empty());
    }

    #[test]
    fn resolve_shell_command_parses_json_object() {
        let env = resolve_shell_command(r#"echo '{"A":"1","B":2,"C":true}'"#).unwrap();
        assert_eq!(env.get("A").unwrap(), "1");
        // non-string JSON values are stringified
        assert_eq!(env.get("B").unwrap(), "2");
        assert_eq!(env.get("C").unwrap(), "true");
    }

    #[test]
    fn resolve_shell_command_falls_back_to_key_value() {
        let env = resolve_shell_command("printf 'K1=v1\\nK2=v2\\n'").unwrap();
        assert_eq!(env.get("K1").unwrap(), "v1");
        assert_eq!(env.get("K2").unwrap(), "v2");
    }

    #[test]
    fn resolve_shell_command_invalid_json_falls_back_to_key_value() {
        // Starts with '{' but is not valid JSON -> falls through to KEY=VALUE.
        let env = resolve_shell_command("echo '{not json=oops'").unwrap();
        // The line "{not json=oops" splits on first '=' to key "{not json" value "oops"
        assert_eq!(env.get("{not json").unwrap(), "oops");
    }

    #[test]
    fn resolve_shell_command_reports_failure() {
        let err = resolve_shell_command("echo boom >&2; exit 3").unwrap_err();
        match err {
            WbError::Secret(msg) => assert!(msg.contains("failed")),
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn resolve_command_requires_command_field() {
        let p = provider("command");
        let err = resolve_command(&p).unwrap_err();
        assert!(matches!(err, WbError::Secret(_)));
    }

    #[test]
    fn resolve_command_runs_given_command() {
        let mut p = provider("cmd");
        p.command = Some("echo TOK=abc".to_string());
        let env = resolve_command(&p).unwrap();
        assert_eq!(env.get("TOK").unwrap(), "abc");
    }

    #[test]
    fn resolve_yard_uses_command_override() {
        let mut p = provider("yard");
        p.command = Some("echo YK=yv".to_string());
        let env = resolve_yard(&p).unwrap();
        assert_eq!(env.get("YK").unwrap(), "yv");
    }

    #[test]
    fn resolve_prompt_requires_keys() {
        let p = provider("prompt");
        let err = resolve_prompt(&p).unwrap_err();
        assert!(matches!(err, WbError::Secret(_)));
    }

    #[test]
    fn load_env_file_reads_and_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.env");
        std::fs::write(&path, "X=1\nY=2\n").unwrap();
        let env = load_env_file(path.to_str().unwrap()).unwrap();
        assert_eq!(env.get("X").unwrap(), "1");

        let err = load_env_file("/no/such/path/.env").unwrap_err();
        assert!(matches!(err, WbError::Secret(_)));
    }

    #[test]
    fn resolve_dotenv_default_and_explicit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("custom.env");
        std::fs::write(&path, "D=dv\n").unwrap();
        let mut p = provider("dotenv");
        p.command = Some(path.to_str().unwrap().to_string());
        let env = resolve_dotenv(&p).unwrap();
        assert_eq!(env.get("D").unwrap(), "dv");
    }

    #[test]
    fn resolve_provider_unknown_errors() {
        let p = provider("bogus");
        let err = resolve_provider(&p).unwrap_err();
        match err {
            WbError::Secret(msg) => assert!(msg.contains("Unknown secret provider")),
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn resolve_provider_dispatches_file_and_command() {
        // file dispatch
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        std::fs::write(&path, "F=fv\n").unwrap();
        let mut fp = provider("file");
        fp.command = Some(path.to_str().unwrap().to_string());
        assert_eq!(resolve_provider(&fp).unwrap().get("F").unwrap(), "fv");

        // command dispatch
        let mut cp = provider("command");
        cp.command = Some("echo G=gv".to_string());
        assert_eq!(resolve_provider(&cp).unwrap().get("G").unwrap(), "gv");
    }

    #[test]
    fn resolve_secrets_single_and_multiple() {
        let mut a = provider("command");
        a.command = Some("echo A=1".to_string());
        let single = resolve_secrets(&SecretsConfig::Single(a.clone())).unwrap();
        assert_eq!(single.get("A").unwrap(), "1");

        let mut b = provider("command");
        b.command = Some("echo B=2".to_string());
        let multi = resolve_secrets(&SecretsConfig::Multiple(vec![a, b])).unwrap();
        assert_eq!(multi.get("A").unwrap(), "1");
        assert_eq!(multi.get("B").unwrap(), "2");
    }

    #[test]
    fn resolve_doppler_unauthenticated_hits_status_failure_branch() {
        // Exercise the `!output.status.success()` arm (lines 66-68): doppler is
        // installed but, forced to use an invalid token + a nonexistent
        // project, the CLI exits non-zero. Bounded and self-skipping so it
        // never hangs or prompts: skip when doppler isn't on PATH, and skip if
        // the call doesn't complete quickly.
        let on_path = Command::new("doppler")
            .arg("--version")
            .output()
            .map(|o| o.status.success() || !o.stdout.is_empty())
            .unwrap_or(false);
        if !on_path {
            eprintln!("skipping doppler test: doppler not on PATH");
            return;
        }

        // Force a fast, non-interactive failure. DOPPLER_TOKEN takes precedence
        // over any configured login; restore it afterward.
        let prev_token = std::env::var_os("DOPPLER_TOKEN");
        std::env::set_var("DOPPLER_TOKEN", "dp.ct.invalidinvalidinvalidinvalid");

        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let mut p = SecretProvider {
                provider: "doppler".to_string(),
                project: Some("wb-nonexistent-project-xyz-99999".to_string()),
                command: None,
                keys: None,
            };
            p.command = None;
            let _ = tx.send(resolve_doppler(&p));
        });

        let result = rx.recv_timeout(std::time::Duration::from_secs(30));

        match prev_token {
            Some(t) => std::env::set_var("DOPPLER_TOKEN", t),
            None => std::env::remove_var("DOPPLER_TOKEN"),
        }

        match result {
            Ok(Ok(_)) => {
                // Unexpected success (e.g. a real token leaked through); don't
                // fail the suite over an environment quirk.
                eprintln!("skipping doppler assertion: call unexpectedly succeeded");
            }
            Ok(Err(e)) => {
                let _ = handle.join();
                let msg = format!("{e:?}");
                assert!(
                    msg.contains("doppler failed") || msg.contains("Failed to run doppler"),
                    "unexpected doppler error: {msg}"
                );
            }
            Err(_) => {
                // Timed out (hung/prompting). Skip rather than fail; the worker
                // thread is detached.
                eprintln!("skipping doppler test: call did not complete in time");
            }
        }
    }
}
