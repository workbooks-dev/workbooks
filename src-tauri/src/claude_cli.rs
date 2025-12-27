use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeInstallInfo {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeSession {
    pub session_id: String,
    pub project_root: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeResponse {
    pub result: String,
    pub session_id: Option<String>,
    pub usage: Option<ClaudeUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingChange {
    pub id: String,
    pub tool: String,
    pub description: String,
    pub file_path: Option<String>,
    pub action: String,
}

/// Check if Claude Code CLI is installed
pub fn check_installation() -> ClaudeInstallInfo {
    // Check if claude command exists
    let which_output = Command::new("which")
        .arg("claude")
        .output();

    if let Ok(output) = which_output {
        if !output.status.success() {
            return ClaudeInstallInfo {
                installed: false,
                version: None,
                path: None,
            };
        }

        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Get version
        let version_output = Command::new("claude")
            .arg("--version")
            .output();

        let version = version_output
            .ok()
            .and_then(|v| String::from_utf8(v.stdout).ok())
            .map(|v| v.trim().to_string());

        ClaudeInstallInfo {
            installed: true,
            version,
            path: Some(path),
        }
    } else {
        ClaudeInstallInfo {
            installed: false,
            version: None,
            path: None,
        }
    }
}

/// Run Claude Code CLI in plan mode (analysis only, no execution)
pub async fn run_plan_mode(
    prompt: &str,
    project_root: &Path,
    session_id: Option<&str>,
) -> Result<(ClaudeResponse, Vec<PendingChange>), String> {
    let mut cmd = TokioCommand::new("claude");
    cmd.arg("-p")
        .arg(prompt)
        .arg("--output-format")
        .arg("json")
        .arg("--permission-mode")
        .arg("plan")
        .current_dir(project_root);

    if let Some(sid) = session_id {
        cmd.arg("--session-id").arg(sid);
    }

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run Claude CLI: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Claude CLI error: {}", stderr));
    }

    let response: ClaudeResponse = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse Claude response: {}", e))?;

    // Parse the response to extract pending changes
    let pending_changes = extract_pending_changes(&response.result);

    Ok((response, pending_changes))
}

/// Run Claude Code CLI with approved tools
pub async fn run_with_approval(
    prompt: &str,
    project_root: &Path,
    session_id: &str,
    allowed_tools: &[String],
) -> Result<ClaudeResponse, String> {
    let mut cmd = TokioCommand::new("claude");
    cmd.arg("-p")
        .arg(prompt)
        .arg("--output-format")
        .arg("json")
        .arg("--session-id")
        .arg(session_id)
        .current_dir(project_root);

    if !allowed_tools.is_empty() {
        cmd.arg("--allowedTools").arg(allowed_tools.join(","));
    }

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run Claude CLI: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Claude CLI error: {}", stderr));
    }

    let response: ClaudeResponse = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse Claude response: {}", e))?;

    Ok(response)
}

/// Run Claude Code CLI with streaming output
pub async fn run_streaming<F>(
    prompt: &str,
    project_root: &Path,
    session_id: Option<&str>,
    allowed_tools: Option<&[String]>,
    mut on_chunk: F,
) -> Result<ClaudeResponse, String>
where
    F: FnMut(String) -> (),
{
    let mut cmd = TokioCommand::new("claude");
    cmd.arg("-p")
        .arg(prompt)
        .arg("--output-format")
        .arg("stream-json")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(project_root);

    if let Some(sid) = session_id {
        cmd.arg("--session-id").arg(sid);
    }

    if let Some(tools) = allowed_tools {
        if !tools.is_empty() {
            cmd.arg("--allowedTools").arg(tools.join(","));
        }
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn Claude CLI: {}", e))?;

    let stdout = child
        .stdout
        .take()
        .ok_or("Failed to capture stdout")?;

    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    let mut full_result = String::new();
    let mut final_session_id: Option<String> = None;
    let mut usage: Option<ClaudeUsage> = None;

    while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
        if line.trim().is_empty() {
            continue;
        }

        // Parse each JSON line
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
            // Handle different event types
            if let Some(event_type) = json.get("type").and_then(|t| t.as_str()) {
                match event_type {
                    "content" => {
                        if let Some(content) = json.get("content").and_then(|c| c.as_str()) {
                            full_result.push_str(content);
                            on_chunk(content.to_string());
                        }
                    }
                    "metadata" => {
                        if let Some(sid) = json.get("session_id").and_then(|s| s.as_str()) {
                            final_session_id = Some(sid.to_string());
                        }
                        if let Some(usage_obj) = json.get("usage") {
                            usage = serde_json::from_value(usage_obj.clone()).ok();
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed to wait for child process: {}", e))?;

    if !status.success() {
        return Err("Claude CLI exited with error".to_string());
    }

    Ok(ClaudeResponse {
        result: full_result,
        session_id: final_session_id,
        usage,
    })
}

/// Extract pending changes from Claude's plan mode response
fn extract_pending_changes(response: &str) -> Vec<PendingChange> {
    let mut changes = Vec::new();

    // Look for tool use patterns in the response
    // This is a simplified parser - Claude's response may contain structured info about planned actions

    // Example patterns to detect:
    // - "I will edit file X to Y"
    // - "I will run command Z"
    // - "I will create file A"

    if response.contains("edit") || response.contains("Edit") {
        changes.push(PendingChange {
            id: Uuid::new_v4().to_string(),
            tool: "Edit".to_string(),
            description: "File edits planned".to_string(),
            file_path: None,
            action: "edit".to_string(),
        });
    }

    if response.contains("write") || response.contains("Write") || response.contains("create") {
        changes.push(PendingChange {
            id: Uuid::new_v4().to_string(),
            tool: "Write".to_string(),
            description: "New files to be created".to_string(),
            file_path: None,
            action: "write".to_string(),
        });
    }

    if response.contains("run") || response.contains("execute") || response.contains("Bash") {
        changes.push(PendingChange {
            id: Uuid::new_v4().to_string(),
            tool: "Bash".to_string(),
            description: "Commands to be executed".to_string(),
            file_path: None,
            action: "bash".to_string(),
        });
    }

    changes
}

/// Continue the most recent session
pub async fn continue_last_session(
    prompt: &str,
    project_root: &Path,
) -> Result<ClaudeResponse, String> {
    let mut cmd = TokioCommand::new("claude");
    cmd.arg("-p")
        .arg(prompt)
        .arg("--continue")
        .arg("--output-format")
        .arg("json")
        .current_dir(project_root);

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run Claude CLI: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Claude CLI error: {}", stderr));
    }

    let response: ClaudeResponse = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse Claude response: {}", e))?;

    Ok(response)
}

/// Get or create a session ID for a project
pub fn get_or_create_session_id(project_root: &Path) -> Result<String, String> {
    let session_file = project_root.join(".workbooks").join("claude_session");

    // Create .workbooks directory if it doesn't exist
    if let Some(parent) = session_file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create .workbooks directory: {}", e))?;
    }

    // Try to read existing session ID
    if session_file.exists() {
        if let Ok(session_id) = std::fs::read_to_string(&session_file) {
            let trimmed = session_id.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
    }

    // Create new session ID
    let session_id = Uuid::new_v4().to_string();
    std::fs::write(&session_file, &session_id)
        .map_err(|e| format!("Failed to write session ID: {}", e))?;

    Ok(session_id)
}
