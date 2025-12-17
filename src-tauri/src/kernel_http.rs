use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::process::{Child, Command, Stdio};

const KERNEL_SERVER_PORT: u16 = 8765;

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

/// Result of executing code in a kernel
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub outputs: Vec<CellOutput>,
}

/// Manages the FastAPI kernel server process
pub struct KernelServer {
    process: Option<Child>,
    pub port: u16,
}

// Implement Send for KernelServer since we're managing the process carefully
unsafe impl Send for KernelServer {}

impl KernelServer {
    /// Start the kernel server
    pub async fn start() -> Result<Self> {
        // Find the kernel_server.py script
        let cwd = std::env::current_dir()?;
        let exe_path = std::env::current_exe()?;
        let exe_dir = exe_path.parent().context("Failed to get executable directory")?;

        let possible_paths = vec![
            // Dev mode
            cwd.join("src-tauri/kernel_server.py"),
            // Production - macOS
            exe_dir.join("../Resources/kernel_server.py"),
            // Production - Windows/Linux
            exe_dir.join("kernel_server.py"),
        ];

        let kernel_script = possible_paths
            .iter()
            .find(|p| p.exists())
            .ok_or_else(|| anyhow::anyhow!("Could not find kernel_server.py. Searched: {:?}", possible_paths))?;

        println!("Found kernel_server.py at: {:?}", kernel_script);

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

        // Start the FastAPI server
        let process = Command::new(&tether_python)
            .arg(kernel_script)
            .arg(KERNEL_SERVER_PORT.to_string())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .context("Failed to start kernel server")?;

        println!("Kernel server started with PID: {:?}", process.id());

        // Wait for server to be ready
        tokio::time::sleep(std::time::Duration::from_millis(2000)).await;

        // Check if server is up
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/health", KERNEL_SERVER_PORT);
        match client.get(&url).timeout(std::time::Duration::from_secs(5)).send().await {
            Ok(response) if response.status().is_success() => {
                println!("Kernel server is healthy");
            }
            Ok(response) => {
                anyhow::bail!("Kernel server returned status: {}", response.status());
            }
            Err(e) => {
                anyhow::bail!("Kernel server health check failed: {}", e);
            }
        }

        Ok(Self {
            process: Some(process),
            port: KERNEL_SERVER_PORT,
        })
    }

    /// Start a kernel for a notebook (static method to avoid Send issues)
    pub async fn start_kernel_http(port: u16, notebook_path: &str, project_root: &Path) -> Result<()> {
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/kernel/start", port);

        let response = client
            .post(&url)
            .json(&serde_json::json!({
                "notebook_path": notebook_path,
                "project_root": project_root.to_string_lossy(),
                "kernel_name": "python3"
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            anyhow::bail!("Failed to start kernel: {}", error_text);
        }

        println!("Kernel started for notebook: {}", notebook_path);
        Ok(())
    }

    /// Execute code in a notebook's kernel (static method to avoid Send issues)
    pub async fn execute_http(port: u16, notebook_path: &str, code: &str) -> Result<ExecutionResult> {
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/kernel/execute", port);

        let response = client
            .post(&url)
            .json(&serde_json::json!({
                "notebook_path": notebook_path,
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

    /// Stop a notebook's kernel (static method to avoid Send issues)
    pub async fn stop_kernel_http(port: u16, notebook_path: &str) -> Result<()> {
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/kernel/stop", port);

        let _response = client
            .post(&url)
            .query(&[("notebook_path", notebook_path)])
            .send()
            .await?;

        println!("Kernel stopped for notebook: {}", notebook_path);
        Ok(())
    }

    /// Shutdown the server
    pub fn shutdown(mut self) -> Result<()> {
        println!("Shutting down kernel server");
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
            let _ = process.wait();
        }
        Ok(())
    }
}
