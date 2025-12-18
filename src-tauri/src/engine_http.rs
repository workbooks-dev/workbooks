use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use futures_util::StreamExt;
use once_cell::sync::Lazy;

// Global HTTP client reused across all requests
static HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .expect("Failed to create HTTP client")
});

// List of uncommon ports to try for the engine server
// Chosen to avoid common dev ports (3000, 8000, 8080, etc.)
const CANDIDATE_PORTS: &[u16] = &[
    18765, // Original + 10000
    28765, 38765, 48765, // Variations
    19234, 29234, 39234, // Different base
    17654, 27654, 37654, // Another set
];

/// Check if a port is available by trying to bind to it
fn is_port_available(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Cell output types matching Jupyter notebook format
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "output_type")]
pub enum CellOutput {
    #[serde(rename = "stream")]
    Stream {
        name: String,
        text: String,
    },
    #[serde(rename = "execute_result")]
    ExecuteResult {
        data: Value,
        execution_count: i32,
    },
    #[serde(rename = "display_data")]
    DisplayData {
        data: Value,
    },
    #[serde(rename = "error")]
    Error {
        ename: String,
        evalue: String,
        traceback: Vec<String>,
    },
}

/// Result of executing code in an engine
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub outputs: Vec<CellOutput>,
}

/// Manages the FastAPI engine server process
pub struct EngineServer {
    process: Option<Child>,
    pub port: u16,
}

// Implement Send for EngineServer since we're managing the process carefully
unsafe impl Send for EngineServer {}

impl EngineServer {
    /// Start the engine server on the first available port
    pub async fn start() -> Result<Self> {
        // Find the engine_server.py script
        let cwd = std::env::current_dir()?;
        let exe_path = std::env::current_exe()?;
        let exe_dir = exe_path.parent().context("Failed to get executable directory")?;

        let possible_paths = vec![
            // Dev mode
            cwd.join("src-tauri/engine_server.py"),
            // Production - macOS
            exe_dir.join("../Resources/engine_server.py"),
            // Production - Windows/Linux
            exe_dir.join("engine_server.py"),
        ];

        let engine_script = possible_paths
            .iter()
            .find(|p| p.exists())
            .ok_or_else(|| anyhow::anyhow!("Could not find engine_server.py. Searched: {:?}", possible_paths))?;

        println!("Found engine_server.py at: {:?}", engine_script);

        // Find Tether's Python executable
        let tether_python = {
            // Try dev mode first - venv is in parent directory
            let dev_venv = if cwd.ends_with("src-tauri") {
                cwd.parent().map(|p| p.join(".venv"))
            } else {
                Some(cwd.join(".venv"))
            };

            if let Some(venv_path) = dev_venv {
                if venv_path.exists() {
                    #[cfg(target_os = "windows")]
                    {
                        venv_path.join("Scripts").join("python.exe")
                    }
                    #[cfg(not(target_os = "windows"))]
                    {
                        venv_path.join("bin").join("python")
                    }
                } else {
                    // Production - TODO: bundle Python or use system Python
                    which::which("python3")
                        .or_else(|_| which::which("python"))
                        .context("Python not found in venv or system")?
                }
            } else {
                which::which("python3")
                    .or_else(|_| which::which("python"))
                    .context("Python not found")?
            }
        };

        println!("Using Python: {:?}", tether_python);

        if !tether_python.exists() {
            anyhow::bail!("Python executable not found at {:?}", tether_python);
        }

        // Try each port until we find an available one
        let mut last_error = None;
        for &port in CANDIDATE_PORTS {
            if !is_port_available(port) {
                println!("Port {} is in use, trying next...", port);
                continue;
            }

            println!("Attempting to start engine server on port {}...", port);

            // Start the FastAPI server
            let process = Command::new(&tether_python)
                .arg(engine_script)
                .arg(port.to_string())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .context("Failed to start engine server")?;

            println!("Engine server started with PID: {} on port {}", process.id(), port);

            // Poll for server readiness with exponential backoff
            let url = format!("http://127.0.0.1:{}/health", port);
            let mut retry_delay = 100; // Start with 100ms
            let max_retries = 15; // Total ~5 seconds (100+150+225+...≈5000ms)

            for attempt in 0..max_retries {
                tokio::time::sleep(std::time::Duration::from_millis(retry_delay)).await;

                match HTTP_CLIENT.get(&url).timeout(std::time::Duration::from_secs(2)).send().await {
                    Ok(response) if response.status().is_success() => {
                        println!("Engine server is healthy on port {} (attempt {})", port, attempt + 1);
                        return Ok(Self {
                            process: Some(process),
                            port,
                        });
                    }
                    Ok(response) => {
                        last_error = Some(format!("Engine server returned status: {}", response.status()));
                    }
                    Err(e) if attempt == max_retries - 1 => {
                        // Only log error on final attempt
                        last_error = Some(format!("Engine server health check failed: {}", e));
                    }
                    _ => {
                        // Continue retrying
                    }
                }

                // Exponential backoff: increase delay by 50% each time
                retry_delay = (retry_delay as f64 * 1.5) as u64;
            }

            // If health check failed, kill the process and try next port
            println!("Port {} failed health check, trying next port...", port);
        }

        // If we get here, all ports failed
        anyhow::bail!(
            "Failed to start engine server on any available port. Last error: {}",
            last_error.unwrap_or_else(|| "All ports in use".to_string())
        )
    }

    /// Start an engine for a workbook (static method to avoid Send issues)
    pub async fn start_engine_http(port: u16, workbook_path: &str, project_root: &Path) -> Result<()> {
        let url = format!("http://127.0.0.1:{}/engine/start", port);

        let response = HTTP_CLIENT
            .post(&url)
            .json(&serde_json::json!({
                "workbook_path": workbook_path,
                "project_root": project_root.to_string_lossy(),
                "engine_name": "python3"
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("Failed to start engine: {}", error_text);
        }

        println!("Engine started for workbook: {}", workbook_path);
        Ok(())
    }

    /// Execute code in a workbook's engine (static method to avoid Send issues)
    pub async fn execute_http(port: u16, workbook_path: &str, code: &str) -> Result<ExecutionResult> {
        let url = format!("http://127.0.0.1:{}/engine/execute", port);

        let response = HTTP_CLIENT
            .post(&url)
            .json(&serde_json::json!({
                "workbook_path": workbook_path,
                "code": code
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("Execution failed: {}", error_text);
        }

        let result: ExecutionResult = response.json().await?;
        Ok(result)
    }

    /// Execute code with streaming output (SSE)
    pub async fn execute_stream<F>(port: u16, workbook_path: &str, code: &str, mut on_output: F) -> Result<bool>
    where
        F: FnMut(CellOutput) + Send,
    {
        let url = format!("http://127.0.0.1:{}/engine/execute_stream", port);

        let response = HTTP_CLIENT
            .post(&url)
            .json(&serde_json::json!({
                "workbook_path": workbook_path,
                "code": code
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("Execution failed: {}", error_text);
        }

        // Process SSE stream
        let mut bytes_stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut success = true;

        while let Some(chunk) = bytes_stream.next().await {
            let chunk = chunk?;
            let text = String::from_utf8_lossy(&chunk);
            buffer.push_str(&text);

            // Process complete SSE messages (separated by \n\n)
            while let Some(pos) = buffer.find("\n\n") {
                let message = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();

                // Parse SSE message
                if let Some(data) = message.strip_prefix("data: ") {
                    if let Ok(event) = serde_json::from_str::<Value>(data) {
                        match event.get("type").and_then(|t| t.as_str()) {
                            Some("output") => {
                                if let Some(output) = event.get("output") {
                                    if let Ok(cell_output) = serde_json::from_value::<CellOutput>(output.clone()) {
                                        on_output(cell_output);
                                    }
                                }
                            }
                            Some("complete") => {
                                success = event.get("success").and_then(|s| s.as_bool()).unwrap_or(true);
                                break;
                            }
                            Some("error") => {
                                success = false;
                                if let Some(msg) = event.get("message").and_then(|m| m.as_str()) {
                                    anyhow::bail!("Execution error: {}", msg);
                                }
                            }
                            Some("timeout") => {
                                success = false;
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        Ok(success)
    }

    /// Stop a workbook's engine (static method to avoid Send issues)
    pub async fn stop_engine_http(port: u16, workbook_path: &str) -> Result<()> {
        let url = format!("http://127.0.0.1:{}/engine/stop", port);

        let _response = HTTP_CLIENT
            .post(&url)
            .query(&[("workbook_path", workbook_path)])
            .send()
            .await?;

        println!("Engine stopped for workbook: {}", workbook_path);
        Ok(())
    }

    /// Interrupt a workbook's currently executing cell
    pub async fn interrupt_engine_http(port: u16, workbook_path: &str) -> Result<()> {
        let url = format!("http://127.0.0.1:{}/engine/interrupt", port);

        let response = HTTP_CLIENT
            .post(&url)
            .query(&[("workbook_path", workbook_path)])
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("Failed to interrupt engine: {}", error_text);
        }

        println!("Engine interrupted for workbook: {}", workbook_path);
        Ok(())
    }

    /// Restart a workbook's engine (static method to avoid Send issues)
    pub async fn restart_engine_http(port: u16, workbook_path: &str, project_root: &Path) -> Result<()> {
        let url = format!("http://127.0.0.1:{}/engine/restart", port);

        let response = HTTP_CLIENT
            .post(&url)
            .json(&serde_json::json!({
                "workbook_path": workbook_path,
                "project_root": project_root.to_string_lossy(),
                "engine_name": "python3"
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("Failed to restart engine: {}", error_text);
        }

        println!("Engine restarted for workbook: {}", workbook_path);
        Ok(())
    }

    /// Shutdown the server
    pub fn shutdown(mut self) -> Result<()> {
        println!("Shutting down engine server");
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
            let _ = process.wait();
        }
        Ok(())
    }
}
