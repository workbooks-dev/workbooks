use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

// This version comes from Cargo.toml
const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Install the workbooks CLI to the system PATH
#[tauri::command]
pub async fn install_cli(app: AppHandle) -> Result<String, String> {
    match install_cli_internal(&app).await {
        Ok(path) => Ok(format!("CLI installed successfully to: {}", path.display())),
        Err(e) => Err(format!("Failed to install CLI: {}", e)),
    }
}

/// Check if the workbooks CLI is already installed and in PATH
#[tauri::command]
pub async fn check_cli_installed() -> Result<bool, String> {
    // Check if 'workbooks' command is available
    if let Ok(_) = which::which("workbooks") {
        return Ok(true);
    }
    Ok(false)
}

/// Get the version of the bundled CLI
#[tauri::command]
pub async fn get_bundled_cli_version() -> Result<String, String> {
    Ok(CLI_VERSION.to_string())
}

/// Get the version of the installed CLI (if any)
#[tauri::command]
pub async fn get_installed_cli_version() -> Result<Option<String>, String> {
    // Try to run `workbooks --version` to get the installed version
    if let Ok(workbooks_path) = which::which("workbooks") {
        let output = std::process::Command::new(workbooks_path)
            .arg("--version")
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let version_str = String::from_utf8_lossy(&output.stdout);
                // Parse "workbooks 0.0.1" to get just "0.0.1"
                if let Some(version) = version_str.split_whitespace().nth(1) {
                    return Ok(Some(version.trim().to_string()));
                }
            }
        }
    }

    Ok(None)
}

async fn install_cli_internal(app: &AppHandle) -> Result<PathBuf> {
    // Get the bundled CLI binary path
    let resource_path = app
        .path()
        .resource_dir()
        .context("Failed to get resource directory")?;

    // The bundled CLI binary is named workbooks-cli (to avoid conflicts with the GUI binary)
    let bundled_cli_name = if cfg!(windows) {
        "workbooks-cli.exe"
    } else {
        "workbooks-cli"
    };

    // Try multiple possible locations for the CLI binary
    let possible_paths = vec![
        resource_path.join(bundled_cli_name),
        resource_path.join("target").join("debug").join(bundled_cli_name), // Debug build
        resource_path.join("target").join("release").join(bundled_cli_name), // Release build
        resource_path.join("..").join(bundled_cli_name), // Sometimes resources are one level up
    ];

    let bundled_cli = possible_paths
        .iter()
        .find(|p| p.exists())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "CLI binary not found in app bundle. Checked: {:?}",
                possible_paths
            )
        })?
        .clone();

    // Determine installation path based on OS
    let install_path = get_install_path()?;

    // Ensure parent directory exists
    if let Some(parent) = install_path.parent() {
        fs::create_dir_all(parent)
            .context("Failed to create installation directory")?;
    }

    // Copy the binary
    fs::copy(&bundled_cli, &install_path)
        .context("Failed to copy CLI binary")?;

    // Make it executable on Unix systems
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&install_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&install_path, perms)?;
    }

    // Automatically add to PATH on Unix systems
    #[cfg(unix)]
    {
        let _ = add_to_path_unix(&install_path);
    }

    Ok(install_path)
}

/// Automatically add installation directory to shell rc files (Unix only)
#[cfg(unix)]
fn add_to_path_unix(install_path: &Path) -> Result<()> {
    let install_dir = install_path.parent().ok_or_else(|| {
        anyhow::anyhow!("Failed to get parent directory")
    })?;

    let home = dirs::home_dir().ok_or_else(|| {
        anyhow::anyhow!("Failed to get home directory")
    })?;

    // Determine which shell rc files to update
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let rc_files = if shell.contains("zsh") {
        vec![home.join(".zshrc")]
    } else if shell.contains("bash") {
        vec![home.join(".bashrc"), home.join(".bash_profile")]
    } else {
        vec![home.join(".profile")]
    };

    let path_line = format!("\n# Added by Workbooks\nexport PATH=\"{}:$PATH\"\n", install_dir.display());

    for rc_file in rc_files {
        // Only update if file exists
        if rc_file.exists() {
            // Check if already added
            if let Ok(content) = fs::read_to_string(&rc_file) {
                if content.contains("Added by Workbooks") || content.contains(&install_dir.to_string_lossy().to_string()) {
                    continue; // Already added
                }

                // Append to file
                if let Ok(mut file) = fs::OpenOptions::new().append(true).open(&rc_file) {
                    use std::io::Write;
                    let _ = writeln!(file, "{}", path_line);
                }
            }
        }
    }

    Ok(())
}

fn get_install_path() -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        // Try ~/.local/bin first (doesn't require sudo)
        if let Some(home) = dirs::home_dir() {
            let local_bin = home.join(".local").join("bin").join("workbooks");
            return Ok(local_bin);
        }
        anyhow::bail!("Failed to determine home directory");
    }

    #[cfg(target_os = "linux")]
    {
        // Try ~/.local/bin first
        if let Some(home) = dirs::home_dir() {
            let local_bin = home.join(".local").join("bin").join("workbooks");
            return Ok(local_bin);
        }
        anyhow::bail!("Failed to determine home directory");
    }

    #[cfg(target_os = "windows")]
    {
        // Windows: Install to %LOCALAPPDATA%\Programs\Workbooks\bin\
        if let Some(local_app_data) = dirs::data_local_dir() {
            let install_dir = local_app_data
                .join("Programs")
                .join("Workbooks")
                .join("bin")
                .join("workbooks.exe");
            return Ok(install_dir);
        }
        anyhow::bail!("Failed to determine local app data directory");
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        anyhow::bail!("Unsupported operating system");
    }
}

/// Add installation directory to PATH (returns instructions for user to manually add)
#[tauri::command]
pub async fn get_path_instructions() -> Result<String, String> {
    match get_install_path() {
        Ok(path) => {
            let install_dir = path.parent().unwrap_or(Path::new(""));

            #[cfg(unix)]
            {
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
                let rc_file = if shell.contains("zsh") {
                    "~/.zshrc"
                } else if shell.contains("bash") {
                    "~/.bashrc"
                } else {
                    "~/.profile"
                };

                Ok(format!(
                    "To use 'workbooks' from your terminal, add this to your {}:\n\n  export PATH=\"{}:$PATH\"\n\nThen run:  source {}",
                    rc_file,
                    install_dir.display(),
                    rc_file
                ))
            }

            #[cfg(target_os = "windows")]
            {
                Ok(format!(
                    "To use 'workbooks' from Command Prompt, add this directory to your PATH:\n\n  {}",
                    install_dir.display()
                ))
            }
        }
        Err(e) => Err(format!("Failed to get installation path: {}", e)),
    }
}
