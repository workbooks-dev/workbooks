use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;

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
pub async fn run_plan_mode<F>(
    prompt: &str,
    project_root: &Path,
    session_name: Option<&str>,
    model: Option<&str>,
    mut on_event: F,
) -> Result<(ClaudeResponse, Vec<PendingChange>), String>
where
    F: FnMut(serde_json::Value) -> (),
{
    let mut cmd = TokioCommand::new("claude");
    cmd.arg("-p")
        .arg(prompt)
        .arg("--output-format")
        .arg("stream-json")
        .arg("--include-partial-messages")  // Include all streaming events
        .arg("--verbose")  // Show tool usage
        .arg("--permission-mode")
        .arg("plan")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(project_root);

    // Note: --resume is NOT compatible with plan mode (-p flag)
    // Plan mode always analyzes fresh, session resumption happens during execution

    if let Some(m) = model {
        cmd.arg("--model").arg(m);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn Claude CLI: {}", e))?;

    let stdout = child
        .stdout
        .take()
        .ok_or("Failed to capture stdout")?;

    let stderr = child
        .stderr
        .take()
        .ok_or("Failed to capture stderr")?;

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
            // Emit all events to frontend for processing
            on_event(json.clone());

            // Handle different event types for our internal tracking
            if let Some(event_type) = json.get("type").and_then(|t| t.as_str()) {
                match event_type {
                    "stream_event" => {
                        // Check if this is a content delta event
                        if let Some(event) = json.get("event") {
                            if let Some(inner_type) = event.get("type").and_then(|t| t.as_str()) {
                                if inner_type == "content_block_delta" {
                                    // Extract the text delta
                                    if let Some(delta) = event.get("delta") {
                                        if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                            full_result.push_str(text);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    "result" => {
                        // Final result contains session_id and usage
                        if let Some(sid) = json.get("session_id").and_then(|s| s.as_str()) {
                            final_session_id = Some(sid.to_string());
                        }
                        if let Some(usage_obj) = json.get("usage") {
                            usage = serde_json::from_value(usage_obj.clone()).ok();
                        }
                    }
                    _ => {
                        // All other events are passed through via on_event
                    }
                }
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed to wait for child process: {}", e))?;

    if !status.success() {
        // Read stderr to get the actual error message
        use tokio::io::AsyncReadExt;
        let mut stderr_output = Vec::new();
        let mut stderr_reader = BufReader::new(stderr);
        stderr_reader.read_to_end(&mut stderr_output).await.ok();
        let stderr_str = String::from_utf8_lossy(&stderr_output);

        if stderr_str.trim().is_empty() {
            return Err("Claude CLI exited with error (no error message)".to_string());
        } else {
            return Err(format!("Claude CLI error: {}", stderr_str.trim()));
        }
    }

    let response = ClaudeResponse {
        result: full_result.clone(),
        session_id: final_session_id,
        usage,
    };

    // Parse the response to extract pending changes
    let pending_changes = extract_pending_changes(&full_result);

    Ok((response, pending_changes))
}

/// Run Claude Code CLI with approved tools
pub async fn run_with_approval(
    prompt: &str,
    project_root: &Path,
    session_name: &str,
    allowed_tools: &[String],
    model: Option<&str>,
) -> Result<ClaudeResponse, String> {
    let mut cmd = TokioCommand::new("claude");
    cmd.arg("-p")
        .arg(prompt)
        .arg("--output-format")
        .arg("json")
        .arg("--resume")
        .arg(session_name)
        .current_dir(project_root);

    if !allowed_tools.is_empty() {
        cmd.arg("--allowedTools").arg(allowed_tools.join(","));
    }

    if let Some(m) = model {
        cmd.arg("--model").arg(m);
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
    session_name: Option<&str>,
    allowed_tools: Option<&[String]>,
    model: Option<&str>,
    mut on_event: F,
) -> Result<ClaudeResponse, String>
where
    F: FnMut(serde_json::Value) -> (),
{
    let mut cmd = TokioCommand::new("claude");
    cmd.arg("-p")
        .arg(prompt)
        .arg("--output-format")
        .arg("stream-json")
        .arg("--include-partial-messages")  // Include all streaming events
        .arg("--verbose")  // Show tool usage
        .arg("--permission-mode")
        .arg("bypassPermissions")  // Allow all tools without prompting
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(project_root);

    // Only use --resume if the session_name looks like a valid UUID
    // Claude CLI session IDs are UUIDs, not friendly names
    if let Some(name) = session_name {
        // Simple UUID format check (8-4-4-4-12 hex digits)
        if name.len() == 36 && name.chars().filter(|c| *c == '-').count() == 4 {
            cmd.arg("--resume").arg(name);
        }
    }

    if let Some(tools) = allowed_tools {
        if !tools.is_empty() {
            cmd.arg("--allowedTools").arg(tools.join(","));
        }
    }

    if let Some(m) = model {
        cmd.arg("--model").arg(m);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn Claude CLI: {}", e))?;

    let stdout = child
        .stdout
        .take()
        .ok_or("Failed to capture stdout")?;

    let stderr = child
        .stderr
        .take()
        .ok_or("Failed to capture stderr")?;

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
            // Emit all events to frontend for processing
            on_event(json.clone());

            // Handle different event types for our internal tracking
            if let Some(event_type) = json.get("type").and_then(|t| t.as_str()) {
                match event_type {
                    "stream_event" => {
                        // Check if this is a content delta event
                        if let Some(event) = json.get("event") {
                            if let Some(inner_type) = event.get("type").and_then(|t| t.as_str()) {
                                if inner_type == "content_block_delta" {
                                    // Extract the text delta
                                    if let Some(delta) = event.get("delta") {
                                        if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                            full_result.push_str(text);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    "result" => {
                        // Final result contains session_id and usage
                        if let Some(sid) = json.get("session_id").and_then(|s| s.as_str()) {
                            final_session_id = Some(sid.to_string());
                        }
                        if let Some(usage_obj) = json.get("usage") {
                            usage = serde_json::from_value(usage_obj.clone()).ok();
                        }
                    }
                    _ => {
                        // All other events are passed through via on_event
                    }
                }
            }
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed to wait for child process: {}", e))?;

    if !status.success() {
        // Read stderr to get the actual error message
        use tokio::io::AsyncReadExt;
        let mut stderr_output = Vec::new();
        let mut stderr_reader = BufReader::new(stderr);
        stderr_reader.read_to_end(&mut stderr_output).await.ok();
        let stderr_str = String::from_utf8_lossy(&stderr_output);

        if stderr_str.trim().is_empty() {
            return Err("Claude CLI exited with error (no error message)".to_string());
        } else {
            return Err(format!("Claude CLI error: {}", stderr_str.trim()));
        }
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
    let mut id_counter = 0;

    // Look for tool use patterns in the response
    // This is a simplified parser - Claude's response may contain structured info about planned actions

    // Example patterns to detect:
    // - "I will edit file X to Y"
    // - "I will run command Z"
    // - "I will create file A"

    if response.contains("edit") || response.contains("Edit") {
        id_counter += 1;
        changes.push(PendingChange {
            id: format!("change-{}", id_counter),
            tool: "Edit".to_string(),
            description: "File edits planned".to_string(),
            file_path: None,
            action: "edit".to_string(),
        });
    }

    if response.contains("write") || response.contains("Write") || response.contains("create") {
        id_counter += 1;
        changes.push(PendingChange {
            id: format!("change-{}", id_counter),
            tool: "Write".to_string(),
            description: "New files to be created".to_string(),
            file_path: None,
            action: "write".to_string(),
        });
    }

    if response.contains("run") || response.contains("execute") || response.contains("Bash") {
        id_counter += 1;
        changes.push(PendingChange {
            id: format!("change-{}", id_counter),
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

/// Get or create a session name for a project
/// Returns a friendly session name based on the project path
pub fn get_or_create_session_name(project_root: &Path) -> Result<String, String> {
    // Generate a session name based on the project directory name
    let project_name = project_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workbooks-project");

    // Create a safe session name (alphanumeric and hyphens only)
    let session_name = format!(
        "workbooks-{}",
        project_name
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect::<String>()
            .to_lowercase()
    );

    Ok(session_name)
}
