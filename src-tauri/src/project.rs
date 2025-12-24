use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use serde::{Deserialize, Serialize};

use crate::python;

/// Slugify a project name to be a valid Python package name
/// Rules: start/end with letter or digit, only contain -, _, ., and alphanumeric
fn slugify_package_name(name: &str) -> String {
    let mut slug = String::new();
    let mut prev_was_separator = false;

    for c in name.chars() {
        if c.is_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            prev_was_separator = false;
        } else if !prev_was_separator && !slug.is_empty() {
            // Replace spaces and other chars with dash
            slug.push('-');
            prev_was_separator = true;
        }
    }

    // Remove trailing separator if any
    slug = slug.trim_end_matches('-').to_string();

    // Ensure we have a valid name
    if slug.is_empty() {
        slug = "project".to_string();
    }

    // Ensure it starts with a letter or digit (should be guaranteed by above logic)
    if !slug.chars().next().unwrap().is_alphanumeric() {
        slug = format!("p{}", slug);
    }

    slug
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkbooksProject {
    pub name: String,          // Display name (can have spaces)
    pub package_name: String,  // Slugified name for uv/Python
    pub root: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkbooksShortcut {
    pub version: u32,
    pub name: String,
    pub project_root: String,
}

/// Create a new Workbooks project from scratch
pub async fn create_new_project(project_path: &Path, project_name: &str) -> Result<WorkbooksProject> {
    let project_root = project_path.to_path_buf();
    let package_name = slugify_package_name(project_name);

    println!("Creating new Workbooks project at {:?}", project_root);
    println!("Display name: '{}', Package name: '{}'", project_name, package_name);

    // Create project directory if it doesn't exist
    if !project_root.exists() {
        fs::create_dir_all(&project_root)
            .context("Failed to create project directory")?;
    }

    // Initialize uv project with slugified name
    init_uv_project(&project_root, &package_name).await?;

    // Create Workbooks directory structure
    create_wbs_structure(&project_root)?;

    // Create notebooks directory
    let notebooks_dir = project_root.join("notebooks");
    if !notebooks_dir.exists() {
        fs::create_dir(&notebooks_dir)
            .context("Failed to create notebooks directory")?;
    }

    // Initialize Python environment and install core dependencies
    python::init_project(&project_root, &package_name).await?;

    println!("Workbooks project created successfully");

    Ok(WorkbooksProject {
        name: project_name.to_string(),
        package_name,
        root: project_root,
    })
}

/// Open an existing folder as a Workbooks project (like VS Code's "Open Folder")
pub async fn open_folder(folder_path: &Path) -> Result<WorkbooksProject> {
    let project_root = folder_path.to_path_buf();

    println!("Opening folder as Workbooks project: {:?}", project_root);

    // Verify folder exists
    if !project_root.exists() {
        anyhow::bail!("Folder does not exist: {:?}", project_root);
    }

    if !project_root.is_dir() {
        anyhow::bail!("Path is not a directory: {:?}", project_root);
    }

    // Get package name from pyproject.toml if it exists, otherwise from folder name
    let package_name = get_project_name(&project_root)
        .unwrap_or_else(|_| {
            let folder_name = project_root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            slugify_package_name(&folder_name)
        });

    // Ensure pyproject.toml exists and has wbs dependencies
    let pyproject_path = project_root.join("pyproject.toml");
    if !pyproject_path.exists() {
        // Initialize uv project if pyproject.toml doesn't exist
        init_uv_project(&project_root, &package_name).await?;
    } else {
        ensure_wbs_dependency_group(&pyproject_path)?;
    }

    // Initialize Python environment and sync dependencies
    python::init_project(&project_root, &package_name).await?;

    // Use folder name as display name
    let display_name = project_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    println!("Opened folder as project: {}", display_name);

    Ok(WorkbooksProject {
        name: display_name,
        package_name,
        root: project_root,
    })
}

/// Ensure .workbooks directory structure exists (called lazily when needed)
#[allow(dead_code)]
pub fn ensure_wbs_structure(project_root: &Path) -> Result<()> {
    let wbs_dir = project_root.join(".workbooks");

    // If .workbooks already exists, nothing to do
    if wbs_dir.exists() {
        return Ok(());
    }

    println!("Creating .workbooks directory structure");

    // Create .workbooks directory
    fs::create_dir(&wbs_dir)
        .context("Failed to create .workbooks directory")?;

    // Create subdirectories
    let subdirs = ["state", "runs"];
    for subdir in &subdirs {
        let dir_path = wbs_dir.join(subdir);
        fs::create_dir(&dir_path)
            .with_context(|| format!("Failed to create .workbooks/{} directory", subdir))?;
    }

    // Create config.toml
    let config_path = wbs_dir.join("config.toml");
    let default_config = r#"[project]
version = "1"

[state]
backend = "sqlite"

[execution]
checkpoint_enabled = true
"#;
    fs::write(&config_path, default_config)
        .context("Failed to create config.toml")?;

    println!("Created .workbooks directory structure");
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

    // Ensure workbooks dependency group exists
    ensure_wbs_dependency_group(&pyproject_path)?;

    Ok(())
}

/// Ensure pyproject.toml has the workbooks dependency group
fn ensure_wbs_dependency_group(pyproject_path: &Path) -> Result<()> {
    let content = fs::read_to_string(pyproject_path)
        .context("Failed to read pyproject.toml")?;

    // Check if [dependency-groups] section exists
    if content.contains("[dependency-groups]") {
        // If dependency-groups section already exists, don't modify it
        // This prevents duplicate section headers and respects user's existing configuration
        println!("Dependency groups section already exists, skipping modification");
        return Ok(());
    }

    println!("Adding workbooks dependency group to pyproject.toml");

    // Append the dependency group section
    let wbs_deps = r#"

[dependency-groups]
workbooks = [
    "pip>=24.0.0",
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

    let new_content = format!("{}{}", content, wbs_deps);
    fs::write(pyproject_path, new_content)
        .context("Failed to write pyproject.toml")?;

    println!("Workbooks dependency group added");
    Ok(())
}

/// Create Workbooks directory structure
fn create_wbs_structure(project_root: &Path) -> Result<()> {
    let wbs_dir = project_root.join(".workbooks");

    if !wbs_dir.exists() {
        fs::create_dir(&wbs_dir)
            .context("Failed to create .workbooks directory")?;
    }

    // Create subdirectories
    let subdirs = ["state", "runs"];
    for subdir in &subdirs {
        let dir_path = wbs_dir.join(subdir);
        if !dir_path.exists() {
            fs::create_dir(&dir_path)
                .with_context(|| format!("Failed to create .workbooks/{} directory", subdir))?;
        }
    }

    // Create config.toml
    let config_path = wbs_dir.join("config.toml");
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

    println!("Created .workbooks directory structure");
    Ok(())
}


/// Load a Workbooks project from a path
pub fn load_project(project_path: &Path) -> Result<WorkbooksProject> {
    // Check if path is a .workbooks shortcut file
    if project_path.extension().and_then(|s| s.to_str()) == Some("wbs") {
        return load_project_from_shortcut(project_path);
    }

    // Otherwise treat as project directory
    let project_root = project_path.to_path_buf();

    // Verify it's a Workbooks project
    let wbs_dir = project_root.join(".workbooks");
    if !wbs_dir.exists() {
        anyhow::bail!("Not a Workbooks project: .workbooks directory not found");
    }

    // Try to get package name from pyproject.toml
    let package_name = get_project_name(&project_root)
        .unwrap_or_else(|_| {
            let folder_name = project_root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            slugify_package_name(&folder_name)
        });

    // Use folder name as display name
    let display_name = project_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    Ok(WorkbooksProject {
        name: display_name,
        package_name,
        root: project_root,
    })
}

/// Load project from a .workbooks shortcut file
fn load_project_from_shortcut(shortcut_path: &Path) -> Result<WorkbooksProject> {
    let content = fs::read_to_string(shortcut_path)
        .context("Failed to read .workbooks shortcut file")?;

    let shortcut: WorkbooksShortcut = serde_json::from_str(&content)
        .context("Failed to parse .workbooks shortcut file")?;

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

    // Get package name from pyproject.toml
    let package_name = get_project_name(&project_root)
        .unwrap_or_else(|_| slugify_package_name(&shortcut.name));

    Ok(WorkbooksProject {
        name: shortcut.name,
        package_name,
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
