use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use serde::{Deserialize, Serialize};

use crate::python;

#[derive(Debug, Serialize, Deserialize)]
pub struct TetherProject {
    pub name: String,
    pub root: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TetherShortcut {
    pub version: u32,
    pub name: String,
    pub project_root: String,
}

/// Create a new Tether project from scratch
pub async fn create_new_project(project_path: &Path, project_name: &str) -> Result<TetherProject> {
    let project_root = project_path.to_path_buf();

    println!("Creating new Tether project at {:?}", project_root);

    // Create project directory if it doesn't exist
    if !project_root.exists() {
        fs::create_dir_all(&project_root)
            .context("Failed to create project directory")?;
    }

    // Initialize uv project
    init_uv_project(&project_root, project_name).await?;

    // Create Tether directory structure
    create_tether_structure(&project_root)?;

    // Create notebooks directory
    let notebooks_dir = project_root.join("notebooks");
    if !notebooks_dir.exists() {
        fs::create_dir(&notebooks_dir)
            .context("Failed to create notebooks directory")?;
    }

    // Initialize Python environment and install core dependencies
    python::init_project(&project_root).await?;

    println!("Tether project created successfully");

    Ok(TetherProject {
        name: project_name.to_string(),
        root: project_root,
    })
}

/// Open an existing folder as a Tether project (like VS Code's "Open Folder")
pub async fn open_folder(folder_path: &Path) -> Result<TetherProject> {
    let project_root = folder_path.to_path_buf();

    println!("Opening folder as Tether project: {:?}", project_root);

    // Verify folder exists
    if !project_root.exists() {
        anyhow::bail!("Folder does not exist: {:?}", project_root);
    }

    if !project_root.is_dir() {
        anyhow::bail!("Path is not a directory: {:?}", project_root);
    }

    // Ensure tether dependency group exists in pyproject.toml
    let pyproject_path = project_root.join("pyproject.toml");
    if pyproject_path.exists() {
        ensure_tether_dependency_group(&pyproject_path)?;
    }

    // Initialize Python environment and sync dependencies
    python::init_project(&project_root).await?;

    // Get project name from pyproject.toml if it exists, otherwise from folder name
    let project_name = get_project_name(&project_root)
        .unwrap_or_else(|_| {
            project_root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string()
        });

    println!("Opened folder as project: {}", project_name);

    Ok(TetherProject {
        name: project_name,
        root: project_root,
    })
}

/// Ensure .tether directory structure exists (called lazily when needed)
#[allow(dead_code)]
pub fn ensure_tether_structure(project_root: &Path) -> Result<()> {
    let tether_dir = project_root.join(".tether");

    // If .tether already exists, nothing to do
    if tether_dir.exists() {
        return Ok(());
    }

    println!("Creating .tether directory structure");

    // Create .tether directory
    fs::create_dir(&tether_dir)
        .context("Failed to create .tether directory")?;

    // Create subdirectories
    let subdirs = ["state", "runs"];
    for subdir in &subdirs {
        let dir_path = tether_dir.join(subdir);
        fs::create_dir(&dir_path)
            .with_context(|| format!("Failed to create .tether/{} directory", subdir))?;
    }

    // Create config.toml
    let config_path = tether_dir.join("config.toml");
    let default_config = r#"[project]
version = "1"

[state]
backend = "sqlite"

[execution]
checkpoint_enabled = true
"#;
    fs::write(&config_path, default_config)
        .context("Failed to create config.toml")?;

    println!("Created .tether directory structure");
    Ok(())
}

/// Initialize a uv project with pyproject.toml
async fn init_uv_project(project_root: &Path, project_name: &str) -> Result<()> {
    let pyproject_path = project_root.join("pyproject.toml");

    // Check if pyproject.toml already exists
    let exists = pyproject_path.exists();

    if !exists {
        println!("Initializing uv project");

        // Ensure uv is installed
        let uv_path = python::ensure_uv().await?;

        // Run uv init
        let output = Command::new(&uv_path)
            .args(["init", "--name", project_name, "--no-workspace"])
            .current_dir(project_root)
            .output()
            .context("Failed to run uv init")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("uv init failed: {}", stderr);
        }

        // Clean up files created by uv init that we don't need
        let readme_path = project_root.join("README.md");
        if readme_path.exists() {
            let _ = fs::remove_file(&readme_path);
            println!("Removed README.md");
        }

        // Remove hello.py or main.py if created
        let hello_path = project_root.join("hello.py");
        if hello_path.exists() {
            let _ = fs::remove_file(&hello_path);
            println!("Removed hello.py");
        }

        let main_path = project_root.join("main.py");
        if main_path.exists() {
            let _ = fs::remove_file(&main_path);
            println!("Removed main.py");
        }

        println!("uv project initialized");
    } else {
        println!("pyproject.toml already exists");
    }

    // Ensure tether dependency group exists
    ensure_tether_dependency_group(&pyproject_path)?;

    Ok(())
}

/// Ensure pyproject.toml has the tether dependency group
fn ensure_tether_dependency_group(pyproject_path: &Path) -> Result<()> {
    let content = fs::read_to_string(pyproject_path)
        .context("Failed to read pyproject.toml")?;

    // Check if [dependency-groups] section exists
    if content.contains("[dependency-groups]") && content.contains("tether = [") {
        println!("Tether dependency group already exists");
        return Ok(());
    }

    println!("Adding tether dependency group to pyproject.toml");

    // Append the dependency group section
    let tether_deps = r#"

[dependency-groups]
tether = [
    "jupyter>=1.0.0",
    "jupyter-client>=8.0.0",
    "ipykernel>=6.0.0",
    "ipython>=8.0.0",
    "nbformat>=5.0.0",
    "papermill>=2.0.0",
    "httpx>=0.27.0",
    "polars>=1.0.0",
    "pandas>=2.0.0",
    "requests>=2.31.0",
]
"#;

    let new_content = format!("{}{}", content, tether_deps);
    fs::write(pyproject_path, new_content)
        .context("Failed to write pyproject.toml")?;

    println!("Tether dependency group added");
    Ok(())
}

/// Create Tether directory structure
fn create_tether_structure(project_root: &Path) -> Result<()> {
    let tether_dir = project_root.join(".tether");

    if !tether_dir.exists() {
        fs::create_dir(&tether_dir)
            .context("Failed to create .tether directory")?;
    }

    // Create subdirectories
    let subdirs = ["state", "runs"];
    for subdir in &subdirs {
        let dir_path = tether_dir.join(subdir);
        if !dir_path.exists() {
            fs::create_dir(&dir_path)
                .with_context(|| format!("Failed to create .tether/{} directory", subdir))?;
        }
    }

    // Create config.toml
    let config_path = tether_dir.join("config.toml");
    if !config_path.exists() {
        let default_config = r#"[project]
version = "1"

[state]
backend = "sqlite"

[execution]
checkpoint_enabled = true
"#;
        fs::write(&config_path, default_config)
            .context("Failed to create config.toml")?;
    }

    println!("Created .tether directory structure");
    Ok(())
}


/// Load a Tether project from a path
pub fn load_project(project_path: &Path) -> Result<TetherProject> {
    // Check if path is a .tether shortcut file
    if project_path.extension().and_then(|s| s.to_str()) == Some("tether") {
        return load_project_from_shortcut(project_path);
    }

    // Otherwise treat as project directory
    let project_root = project_path.to_path_buf();

    // Verify it's a Tether project
    let tether_dir = project_root.join(".tether");
    if !tether_dir.exists() {
        anyhow::bail!("Not a Tether project: .tether directory not found");
    }

    // Try to get project name from pyproject.toml
    let project_name = get_project_name(&project_root)
        .unwrap_or_else(|_| {
            project_root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string()
        });

    Ok(TetherProject {
        name: project_name,
        root: project_root,
    })
}

/// Load project from a .tether shortcut file
fn load_project_from_shortcut(shortcut_path: &Path) -> Result<TetherProject> {
    let content = fs::read_to_string(shortcut_path)
        .context("Failed to read .tether shortcut file")?;

    let shortcut: TetherShortcut = serde_json::from_str(&content)
        .context("Failed to parse .tether shortcut file")?;

    let shortcut_dir = shortcut_path.parent()
        .context("Failed to get shortcut directory")?;

    let project_root = if shortcut.project_root == "." {
        shortcut_dir.to_path_buf()
    } else {
        shortcut_dir.join(&shortcut.project_root)
    };

    // Verify project exists
    if !project_root.exists() {
        anyhow::bail!("Project directory not found: {:?}", project_root);
    }

    Ok(TetherProject {
        name: shortcut.name,
        root: project_root,
    })
}

/// Get project name from pyproject.toml
fn get_project_name(project_root: &Path) -> Result<String> {
    let pyproject_path = project_root.join("pyproject.toml");
    let content = fs::read_to_string(pyproject_path)
        .context("Failed to read pyproject.toml")?;

    // Simple TOML parsing for [project] name
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("name") {
            if let Some(name) = trimmed.split('=').nth(1) {
                let name = name.trim().trim_matches('"').trim_matches('\'');
                return Ok(name.to_string());
            }
        }
    }

    anyhow::bail!("Project name not found in pyproject.toml")
}
