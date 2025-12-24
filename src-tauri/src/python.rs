use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use which::which;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Check if uv is available in PATH or common installation locations
pub fn check_uv_available() -> Result<PathBuf> {
    // First try to find uv in PATH
    if let Ok(path) = which("uv") {
        return Ok(path);
    }

    // If not in PATH, check common installation locations
    let home_dir = dirs::home_dir()
        .context("Failed to get home directory")?;

    let common_locations = vec![
        home_dir.join(".cargo").join("bin").join("uv"),
        home_dir.join(".local").join("bin").join("uv"),
        PathBuf::from("/usr/local/bin/uv"),
    ];

    for location in common_locations {
        if location.exists() && location.is_file() {
            println!("Found uv at {:?}", location);
            return Ok(location);
        }
    }

    anyhow::bail!("uv not found in PATH or common installation locations (~/.cargo/bin, ~/.local/bin, /usr/local/bin)")
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

    // Return the path to uv (checks PATH and common locations)
    check_uv_available().context("uv was installed but not found. You may need to restart your terminal or add ~/.cargo/bin or ~/.local/bin to your PATH")
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

    // Return the path to uv (checks PATH and common locations)
    check_uv_available().context("uv was installed but not found. You may need to restart your terminal")
}

/// Ensure uv is available, installing it if necessary
pub async fn ensure_uv() -> Result<PathBuf> {
    match check_uv_available() {
        Ok(path) => {
            // println!("uv found at {:?}", path);
            Ok(path)
        }
        Err(_) => {
            // println!("uv not found, installing...");
            install_uv().await
        }
    }
}

/// Get the centralized venv path for a project
/// Format: ~/.workbooks/venvs/<project-name>-<hash>
pub fn get_venv_path(project_root: &Path, project_name: &str) -> Result<PathBuf> {
    // First, check if project has a local .venv directory (standard Python convention)
    let local_venv = project_root.join(".venv");
    if local_venv.exists() && local_venv.is_dir() {
        // Verify it's a valid venv by checking for bin/python or Scripts/python.exe
        let python_path = if cfg!(target_os = "windows") {
            local_venv.join("Scripts").join("python.exe")
        } else {
            local_venv.join("bin").join("python")
        };

        if python_path.exists() {
            // println!("Using local .venv at {:?}", local_venv);
            return Ok(local_venv);
        }
    }

    // Fall back to centralized venv at ~/.workbooks/venvs/
    // Get home directory
    let home_dir = dirs::home_dir()
        .context("Failed to get home directory")?;

    // Create a hash of the project root path for uniqueness
    let mut hasher = DefaultHasher::new();
    project_root.to_string_lossy().hash(&mut hasher);
    let path_hash = format!("{:x}", hasher.finish());

    // Use first 8 characters of hash for brevity
    let short_hash = &path_hash[..8];

    // Slugify project name for filesystem safety
    let safe_name = project_name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect::<String>()
        .to_lowercase();

    // Build path: ~/.workbooks/venvs/<project-name>-<hash>
    let venv_dir = home_dir
        .join(".workbooks")
        .join("venvs")
        .join(format!("{}-{}", safe_name, short_hash));

    Ok(venv_dir)
}

/// Ensure Python virtual environment exists for a project
pub async fn ensure_venv(project_root: &Path, project_name: &str) -> Result<PathBuf> {
    // Use new intelligent venv resolution
    let venv_path = determine_venv_path(project_root, project_name).await?;

    // For UV-managed projects, run uv sync --group workbooks
    if is_uv_managed_project(project_root) {
        println!("Running uv sync --group workbooks...");
        let uv_path = ensure_uv().await?;
        let output = Command::new(uv_path)
            .args(["sync", "--group", "workbooks"])
            .current_dir(project_root)
            .output()
            .context("Failed to run uv sync")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("Warning: uv sync failed: {}", stderr);
        }
    }

    // If venv doesn't exist yet (workbooks-managed), create it
    if !venv_path.exists() {
        println!("Creating virtual environment at {:?}", venv_path);
        create_venv_at_path(&venv_path).await?;
    }

    Ok(venv_path)
}

/// Ensure ipykernel is installed in the venv (needed for notebook execution)
pub async fn ensure_ipykernel(venv_path: &Path) -> Result<()> {
    let uv_path = ensure_uv().await?;

    // Check if ipykernel is installed
    let python_path = venv_path.join("bin").join("python");
    let check_output = Command::new(&python_path)
        .args(["-c", "import ipykernel"])
        .output();

    if let Ok(output) = check_output {
        if output.status.success() {
            return Ok(());
        }
    }

    // ipykernel not installed, install it
    // println!("Installing ipykernel...");
    let output = Command::new(uv_path)
        .args(["pip", "install", "ipykernel"])
        .env("VIRTUAL_ENV", venv_path)
        .output()
        .context("Failed to install ipykernel")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to install ipykernel: {}", stderr);
    }

    // println!("✓ ipykernel installed");
    Ok(())
}

/// Create a new virtual environment at a specific path using uv
async fn create_venv_at_path(venv_path: &Path) -> Result<()> {
    let uv_path = ensure_uv().await?;

    // Ensure parent directory exists
    if let Some(parent) = venv_path.parent() {
        fs::create_dir_all(parent)
            .context("Failed to create venv parent directory")?;
    }

    let output = Command::new(uv_path)
        .args(["venv", venv_path.to_str().context("Invalid venv path")?])
        .output()
        .context("Failed to create virtual environment")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to create venv: {}", stderr);
    }

    // println!("Virtual environment created successfully at {:?}", venv_path);
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

/// Sync dependencies from pyproject.toml using uv, optionally with a specific group
pub async fn sync_dependencies(project_root: &Path, venv_path: &Path) -> Result<()> {
    sync_dependencies_with_group(project_root, venv_path, Some("workbooks")).await
}

/// Sync dependencies from pyproject.toml using uv with optional group
pub async fn sync_dependencies_with_group(
    project_root: &Path,
    venv_path: &Path,
    group: Option<&str>
) -> Result<()> {
    let uv_path = ensure_uv().await?;
    let pyproject_path = project_root.join("pyproject.toml");

    if !pyproject_path.exists() {
        // println!("No pyproject.toml found, skipping sync");
        return Ok(());
    }

    // if let Some(group_name) = group {
    //     println!("Syncing dependencies with {} group to venv at {:?}", group_name, venv_path);
    // } else {
    //     println!("Syncing dependencies to venv at {:?}", venv_path);
    // }

    // Use UV_PROJECT_ENVIRONMENT to tell uv where to install packages
    // This is the correct way to use a custom venv location with uv sync
    let mut cmd = Command::new(uv_path);
    cmd.arg("sync");

    if let Some(group_name) = group {
        cmd.args(["--group", group_name]);
    }

    let output = cmd
        .current_dir(project_root)
        .env("UV_PROJECT_ENVIRONMENT", venv_path)
        .output()
        .context("Failed to sync dependencies")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        // println!("Sync stderr: {}", stderr);
        // println!("Sync stdout: {}", stdout);
        anyhow::bail!("Failed to sync dependencies: {}", stderr);
    }

    // println!("Dependencies synced successfully");
    Ok(())
}

/// Initialize a Workbooks project with Python environment and core dependencies
pub async fn init_project(project_root: &Path, project_name: &str) -> Result<()> {
    println!("Initializing project: {} at {:?}", project_name, project_root);

    // Ensure venv exists in centralized location
    let venv_path = ensure_venv(project_root, project_name).await?;
    println!("Venv path: {:?}", venv_path);

    // Always sync dependencies with workbooks group
    if project_root.join("pyproject.toml").exists() {
        println!("Found pyproject.toml, syncing dependencies...");
        sync_dependencies(project_root, &venv_path).await?;
        println!("Dependencies sync completed");
    } else {
        println!("Warning: No pyproject.toml found. Dependencies not synced.");
    }

    Ok(())
}

/// Get the Python executable path from the centralized venv
pub fn get_python_path(project_root: &Path, project_name: &str) -> Result<PathBuf> {
    let venv_path = get_venv_path(project_root, project_name)?;

    #[cfg(target_os = "windows")]
    {
        Ok(venv_path.join("Scripts").join("python.exe"))
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(venv_path.join("bin").join("python"))
    }
}

/// Run a Python command in the project's virtual environment
pub async fn run_python_command(project_root: &Path, project_name: &str, args: &[&str]) -> Result<String> {
    let python_path = get_python_path(project_root, project_name)?;

    if !python_path.exists() {
        anyhow::bail!("Python executable not found at {:?}. Please initialize the project first.", python_path);
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

/// Check if project is UV-managed (has uv.lock file)
pub fn is_uv_managed_project(project_root: &Path) -> bool {
    project_root.join("uv.lock").exists()
}

/// Detect if project has Python environment markers
/// Returns true if project has: requirements.txt, Pipfile.lock, venv, .venv, or pyproject.toml (without uv.lock)
pub fn has_user_python_environment(project_root: &Path) -> bool {
    // Check for UV-managed first (if uv.lock exists, it's not a "user" env in the prompt sense)
    if is_uv_managed_project(project_root) {
        return false;
    }

    // Check for various Python environment markers
    let markers = [
        "requirements.txt",
        "Pipfile.lock",
        "venv",
        ".venv",
        "pyproject.toml",
    ];

    markers.iter().any(|marker| project_root.join(marker).exists())
}

/// Find user's existing virtual environment path
/// Checks common locations: .venv, venv, env, .env
pub fn find_user_venv(project_root: &Path) -> Option<PathBuf> {
    let venv_candidates = [".venv", "venv", "env", ".env"];

    for candidate in &venv_candidates {
        let venv_path = project_root.join(candidate);
        if venv_path.exists() && venv_path.is_dir() {
            // Verify it's a valid venv by checking for bin/python or Scripts/python.exe
            let python_path = if cfg!(target_os = "windows") {
                venv_path.join("Scripts").join("python.exe")
            } else {
                venv_path.join("bin").join("python")
            };

            if python_path.exists() {
                return Some(venv_path);
            }
        }
    }

    None
}

/// Prompt user to choose venv strategy (CLI only)
/// Returns the chosen strategy and optional venv path
pub fn prompt_venv_strategy(project_root: &Path) -> Result<(crate::config::VenvStrategy, Option<PathBuf>)> {
    use std::io::{self, Write};

    println!("\n📦 Python Environment Setup");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Workbooks detected an existing Python environment in this project.");
    println!("\nHow would you like Workbooks to manage the virtual environment?\n");
    println!("  1. Let Workbooks manage it (recommended)");
    println!("     → Workbooks creates a centralized venv and handles dependencies");
    println!("\n  2. Use my own virtual environment");
    println!("     → You manage packages with pip/poetry/pipenv");
    println!("     → Workbooks only installs minimal requirements (ipykernel)");
    println!("     → Can be error-prone if dependencies conflict\n");

    print!("Choose [1/2] (default: 1): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let choice = input.trim();

    match choice {
        "2" => {
            // User-managed: find their venv
            if let Some(venv_path) = find_user_venv(project_root) {
                println!("\n✓ Using virtual environment at: {}", venv_path.display());
                Ok((crate::config::VenvStrategy::UserManaged, Some(venv_path)))
            } else {
                println!("\n⚠ No valid virtual environment found in project.");
                println!("Falling back to Workbooks-managed virtual environment.");
                Ok((crate::config::VenvStrategy::WorkbooksManaged, None))
            }
        }
        _ => {
            // Workbooks-managed (default)
            println!("\n✓ Using Workbooks-managed virtual environment");
            Ok((crate::config::VenvStrategy::WorkbooksManaged, None))
        }
    }
}

/// Determine which venv to use based on project markers and user preferences
/// This is the main entry point for venv resolution
pub async fn determine_venv_path(project_root: &Path, project_name: &str) -> Result<PathBuf> {
    // 1. Check if UV-managed project (has uv.lock)
    if is_uv_managed_project(project_root) {
        let local_venv = project_root.join(".venv");
        if local_venv.exists() {
            return Ok(local_venv);
        }
    }

    // 2. Check saved preference in config
    let config = crate::config::load_config(project_root).unwrap_or_default();

    match config.python.venv_strategy {
        crate::config::VenvStrategy::UserManaged => {
            // Use saved user venv path or try to find it
            if let Some(ref venv_path_str) = config.python.venv_path {
                let venv_path = PathBuf::from(venv_path_str);
                if venv_path.exists() {
                    println!("Using saved user-managed venv at {}", venv_path.display());
                    return Ok(venv_path);
                }
            }

            // Saved path doesn't exist, try to find user venv
            if let Some(venv_path) = find_user_venv(project_root) {
                println!("Using user-managed venv at {}", venv_path.display());
                return Ok(venv_path);
            }

            // Can't find user venv, fall back to workbooks-managed
            println!("⚠ Saved user venv not found, falling back to Workbooks-managed");
        }

        crate::config::VenvStrategy::WorkbooksManaged => {
            // Use centralized Workbooks venv
            return get_venv_path(project_root, project_name);
        }

        crate::config::VenvStrategy::Auto => {
            // Auto-detect: prompt user if needed
            if has_user_python_environment(project_root) {
                // Prompt user for preference (CLI only, GUI will handle separately)
                if std::env::var("WORKBOOKS_CLI").is_ok() {
                    let (strategy, venv_path) = prompt_venv_strategy(project_root)?;

                    // Save preference
                    let venv_path_str = venv_path.as_ref().map(|p| p.to_string_lossy().to_string());
                    crate::config::set_venv_strategy(project_root, strategy.clone(), venv_path_str)?;

                    // Return appropriate venv
                    match strategy {
                        crate::config::VenvStrategy::UserManaged => {
                            if let Some(venv) = venv_path {
                                return Ok(venv);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Default: Workbooks-managed centralized venv
    get_venv_path(project_root, project_name)
}

