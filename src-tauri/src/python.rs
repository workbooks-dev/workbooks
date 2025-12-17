use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use which::which;
use std::fs;
use std::os::unix::fs::PermissionsExt;

/// Check if uv is available in PATH
pub fn check_uv_available() -> Result<PathBuf> {
    which("uv").context("uv not found in PATH")
}

/// Install uv using the official installer
pub async fn install_uv() -> Result<PathBuf> {
    println!("Installing uv...");

    #[cfg(target_os = "windows")]
    {
        install_uv_windows().await
    }

    #[cfg(not(target_os = "windows"))]
    {
        install_uv_unix().await
    }
}

#[cfg(not(target_os = "windows"))]
async fn install_uv_unix() -> Result<PathBuf> {
    use std::io::Write;

    // Download the installer script
    let script_url = "https://astral.sh/uv/install.sh";
    let response = reqwest::blocking::get(script_url)
        .context("Failed to download uv installer")?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to download uv installer: HTTP {}", response.status());
    }

    let script_content = response.text()
        .context("Failed to read installer script")?;

    // Save script to temp file
    let temp_dir = std::env::temp_dir();
    let script_path = temp_dir.join("install-uv.sh");

    let mut file = fs::File::create(&script_path)
        .context("Failed to create installer script")?;
    file.write_all(script_content.as_bytes())
        .context("Failed to write installer script")?;

    // Make script executable
    let mut perms = fs::metadata(&script_path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms)?;

    // Run the installer
    let output = Command::new("sh")
        .arg(&script_path)
        .output()
        .context("Failed to run uv installer")?;

    // Clean up
    let _ = fs::remove_file(&script_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("uv installation failed: {}", stderr);
    }

    println!("uv installed successfully");

    // Return the path to uv
    which("uv").context("uv was installed but not found in PATH. You may need to restart your terminal or add ~/.cargo/bin to your PATH")
}

#[cfg(target_os = "windows")]
async fn install_uv_windows() -> Result<PathBuf> {
    use std::io::Write;

    // Download the installer script
    let script_url = "https://astral.sh/uv/install.ps1";
    let response = reqwest::blocking::get(script_url)
        .context("Failed to download uv installer")?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to download uv installer: HTTP {}", response.status());
    }

    let script_content = response.text()
        .context("Failed to read installer script")?;

    // Save script to temp file
    let temp_dir = std::env::temp_dir();
    let script_path = temp_dir.join("install-uv.ps1");

    let mut file = fs::File::create(&script_path)
        .context("Failed to create installer script")?;
    file.write_all(script_content.as_bytes())
        .context("Failed to write installer script")?;

    // Run the installer with PowerShell
    let output = Command::new("powershell")
        .args(["-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script_path)
        .output()
        .context("Failed to run uv installer")?;

    // Clean up
    let _ = fs::remove_file(&script_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("uv installation failed: {}", stderr);
    }

    println!("uv installed successfully");

    // Return the path to uv
    which("uv").context("uv was installed but not found in PATH. You may need to restart your terminal")
}

/// Ensure uv is available, installing it if necessary
pub async fn ensure_uv() -> Result<PathBuf> {
    match check_uv_available() {
        Ok(path) => {
            println!("uv found at {:?}", path);
            Ok(path)
        }
        Err(_) => {
            println!("uv not found, installing...");
            install_uv().await
        }
    }
}

/// Ensure Python virtual environment exists for a project
pub async fn ensure_venv(project_root: &Path) -> Result<PathBuf> {
    let venv_path = project_root.join(".venv");

    if !venv_path.exists() {
        println!("Creating virtual environment at {:?}", venv_path);
        create_venv(project_root).await?;
    } else {
        println!("Virtual environment already exists at {:?}", venv_path);
    }

    Ok(venv_path)
}

/// Create a new virtual environment using uv
async fn create_venv(project_root: &Path) -> Result<()> {
    let uv_path = ensure_uv().await?;

    let output = Command::new(uv_path)
        .args(["venv", ".venv"])
        .current_dir(project_root)
        .output()
        .context("Failed to create virtual environment")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to create venv: {}", stderr);
    }

    println!("Virtual environment created successfully");
    Ok(())
}

/// Install a package using uv
pub async fn install_package(project_root: &Path, package: &str) -> Result<()> {
    let uv_path = ensure_uv().await?;

    println!("Installing package: {}", package);

    let output = Command::new(uv_path)
        .args(["pip", "install", package])
        .current_dir(project_root)
        .output()
        .context("Failed to install package")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to install {}: {}", package, stderr);
    }

    println!("Package {} installed successfully", package);
    Ok(())
}

/// Install multiple packages using uv
pub async fn install_packages(project_root: &Path, packages: &[&str]) -> Result<()> {
    let uv_path = ensure_uv().await?;

    println!("Installing packages: {:?}", packages);

    let mut args = vec!["pip", "install"];
    args.extend(packages);

    let output = Command::new(uv_path)
        .args(&args)
        .current_dir(project_root)
        .output()
        .context("Failed to install packages")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to install packages: {}", stderr);
    }

    println!("Packages installed successfully");
    Ok(())
}

/// Sync dependencies from pyproject.toml using uv with tether group
pub async fn sync_dependencies(project_root: &Path) -> Result<()> {
    let uv_path = ensure_uv().await?;
    let pyproject_path = project_root.join("pyproject.toml");

    if !pyproject_path.exists() {
        println!("No pyproject.toml found, skipping sync");
        return Ok(());
    }

    println!("Syncing dependencies with tether group");

    let output = Command::new(uv_path)
        .args(["sync", "--group", "tether"])
        .current_dir(project_root)
        .output()
        .context("Failed to sync dependencies")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to sync dependencies: {}", stderr);
    }

    println!("Dependencies synced successfully");
    Ok(())
}

/// Initialize a Tether project with Python environment and core dependencies
pub async fn init_project(project_root: &Path) -> Result<()> {
    // Ensure venv exists
    ensure_venv(project_root).await?;

    // Always sync dependencies with tether group
    if project_root.join("pyproject.toml").exists() {
        sync_dependencies(project_root).await?;
    } else {
        println!("Warning: No pyproject.toml found. Dependencies not synced.");
    }

    Ok(())
}

/// Get the Python executable path from the venv
pub fn get_python_path(project_root: &Path) -> PathBuf {
    let venv_path = project_root.join(".venv");

    #[cfg(target_os = "windows")]
    {
        venv_path.join("Scripts").join("python.exe")
    }

    #[cfg(not(target_os = "windows"))]
    {
        venv_path.join("bin").join("python")
    }
}

/// Run a Python command in the project's virtual environment
pub async fn run_python_command(project_root: &Path, args: &[&str]) -> Result<String> {
    let python_path = get_python_path(project_root);

    if !python_path.exists() {
        anyhow::bail!("Python executable not found. Please initialize the project first.");
    }

    let output = Command::new(python_path)
        .args(args)
        .current_dir(project_root)
        .output()
        .context("Failed to run Python command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Python command failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(stdout)
}

