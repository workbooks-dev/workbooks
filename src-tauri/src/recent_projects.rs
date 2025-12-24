use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_RECENT_PROJECTS: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentProject {
    pub name: String,
    pub path: PathBuf,
    pub last_opened: i64, // Unix timestamp
}

#[derive(Debug, Serialize, Deserialize)]
struct RecentProjectsData {
    projects: Vec<RecentProject>,
}

/// Get the path to the recent projects file
fn get_recent_projects_path() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .context("Failed to get home directory")?;

    let workbooks_dir = home.join(".workbooks");
    if !workbooks_dir.exists() {
        fs::create_dir_all(&workbooks_dir)
            .context("Failed to create .workbooks directory")?;
    }

    Ok(workbooks_dir.join("recent_projects.json"))
}

/// Load recent projects from disk
pub fn load_recent_projects() -> Result<Vec<RecentProject>> {
    let path = get_recent_projects_path()?;

    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .context("Failed to read recent projects file")?;

    let data: RecentProjectsData = serde_json::from_str(&content)
        .context("Failed to parse recent projects JSON")?;

    Ok(data.projects)
}

/// Save recent projects to disk
fn save_recent_projects(projects: &[RecentProject]) -> Result<()> {
    let path = get_recent_projects_path()?;

    let data = RecentProjectsData {
        projects: projects.to_vec(),
    };

    let json = serde_json::to_string_pretty(&data)
        .context("Failed to serialize recent projects")?;

    fs::write(&path, json)
        .context("Failed to write recent projects file")?;

    Ok(())
}

/// Add a project to the recent projects list
pub fn add_recent_project(name: &str, path: &Path) -> Result<()> {
    let mut projects = load_recent_projects().unwrap_or_default();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Remove if already exists (we'll re-add it at the front)
    projects.retain(|p| p.path != path);

    // Add new entry at the front
    projects.insert(0, RecentProject {
        name: name.to_string(),
        path: path.to_path_buf(),
        last_opened: now,
    });

    // Keep only the most recent MAX_RECENT_PROJECTS
    projects.truncate(MAX_RECENT_PROJECTS);

    save_recent_projects(&projects)?;

    Ok(())
}

/// Get the list of recent projects (max 3, sorted by most recent)
pub fn get_recent_projects() -> Vec<RecentProject> {
    load_recent_projects().unwrap_or_default()
}
