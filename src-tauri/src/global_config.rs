use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// AI features configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiConfig {
    /// Whether AI features are enabled
    #[serde(default)]
    pub enabled: bool,
}

/// App-level preferences
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// UI theme (dark/light)
    pub theme: Option<String>,
}

/// Global Workbooks configuration
/// Stored at ~/.workbooks/config.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalConfig {
    /// AI features configuration
    #[serde(default)]
    pub ai: AiConfig,

    /// App preferences
    #[serde(default)]
    pub app: AppConfig,

    /// Default project path (for app launch and CLI)
    pub default_project: Option<String>,

    /// Recently opened projects
    #[serde(default)]
    pub recent_projects: Vec<String>,
}

/// Get the path to the global config file
fn get_global_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to get home directory")?;
    let workbooks_dir = home.join(".workbooks");

    // Ensure .workbooks directory exists
    if !workbooks_dir.exists() {
        fs::create_dir_all(&workbooks_dir)
            .context("Failed to create ~/.workbooks directory")?;
    }

    Ok(workbooks_dir.join("config.toml"))
}

/// Load global configuration from ~/.workbooks/config.toml
pub fn load_global_config() -> Result<GlobalConfig> {
    let path = get_global_config_path()?;

    if !path.exists() {
        return Ok(GlobalConfig::default());
    }

    let content = fs::read_to_string(&path)
        .context("Failed to read global config file")?;

    let config: GlobalConfig = toml::from_str(&content)
        .unwrap_or_else(|_| GlobalConfig::default());

    Ok(config)
}

/// Save global configuration to ~/.workbooks/config.toml
pub fn save_global_config(config: &GlobalConfig) -> Result<()> {
    let path = get_global_config_path()?;

    let toml = toml::to_string_pretty(config)
        .context("Failed to serialize config to TOML")?;

    fs::write(&path, toml)
        .context("Failed to write global config file")?;

    // Set restrictive permissions on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&path, perms)
            .context("Failed to set config file permissions")?;
    }

    Ok(())
}

/// Enable or disable AI features
pub fn set_ai_enabled(enabled: bool) -> Result<()> {
    let mut config = load_global_config()?;
    config.ai.enabled = enabled;
    save_global_config(&config)?;
    Ok(())
}

/// Set the default project
pub fn set_default_project(project_path: Option<String>) -> Result<()> {
    let mut config = load_global_config()?;
    config.default_project = project_path;
    save_global_config(&config)?;
    Ok(())
}

/// Add a project to the recent projects list
pub fn add_recent_project(project_path: String) -> Result<()> {
    let mut config = load_global_config()?;

    // Remove if already exists (to move to front)
    config.recent_projects.retain(|p| p != &project_path);

    // Add to front
    config.recent_projects.insert(0, project_path);

    // Limit to 10 most recent
    if config.recent_projects.len() > 10 {
        config.recent_projects.truncate(10);
    }

    save_global_config(&config)?;
    Ok(())
}

// Tauri commands
#[tauri::command]
pub fn get_global_config() -> Result<GlobalConfig, String> {
    load_global_config().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_global_config(config: GlobalConfig) -> Result<(), String> {
    save_global_config(&config).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_ai_features_enabled(enabled: bool) -> Result<(), String> {
    set_ai_enabled(enabled).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_default_project_path(path: Option<String>) -> Result<(), String> {
    set_default_project(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_project_to_recent(path: String) -> Result<(), String> {
    add_recent_project(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_default_project() -> Result<Option<String>, String> {
    load_global_config()
        .map(|c| c.default_project)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_global_recent_projects() -> Result<Vec<String>, String> {
    load_global_config()
        .map(|c| c.recent_projects)
        .map_err(|e| e.to_string())
}
