use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Python virtual environment management strategy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum VenvStrategy {
    /// Let Workbooks manage a centralized virtual environment
    WorkbooksManaged,
    /// Use the user's own virtual environment
    UserManaged,
    /// Auto-detect based on project markers (uv.lock, requirements.txt, etc.)
    Auto,
}

impl Default for VenvStrategy {
    fn default() -> Self {
        VenvStrategy::Auto
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PythonConfig {
    /// Virtual environment strategy
    #[serde(default)]
    pub venv_strategy: VenvStrategy,

    /// Optional: User-specified venv path (for user-managed mode)
    pub venv_path: Option<String>,
}

impl Default for PythonConfig {
    fn default() -> Self {
        PythonConfig {
            venv_strategy: VenvStrategy::Auto,
            venv_path: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkbooksConfig {
    #[serde(default)]
    pub python: PythonConfig,
}

impl Default for WorkbooksConfig {
    fn default() -> Self {
        WorkbooksConfig {
            python: PythonConfig::default(),
        }
    }
}

/// Load Workbooks configuration from .workbooks/config.toml
pub fn load_config(project_root: &Path) -> Result<WorkbooksConfig> {
    let config_path = project_root.join(".workbooks").join("config.toml");

    if !config_path.exists() {
        return Ok(WorkbooksConfig::default());
    }

    let config_str = fs::read_to_string(&config_path)
        .context("Failed to read config.toml")?;

    let config: WorkbooksConfig = toml::from_str(&config_str)
        .unwrap_or_else(|_| WorkbooksConfig::default());

    Ok(config)
}

/// Save Workbooks configuration to .workbooks/config.toml
pub fn save_config(project_root: &Path, config: &WorkbooksConfig) -> Result<()> {
    let workbooks_dir = project_root.join(".workbooks");

    // Ensure .workbooks directory exists
    if !workbooks_dir.exists() {
        fs::create_dir_all(&workbooks_dir)
            .context("Failed to create .workbooks directory")?;
    }

    let config_path = workbooks_dir.join("config.toml");

    // Read existing config to preserve other sections
    let existing_toml: toml::Value = if config_path.exists() {
        let existing_str = fs::read_to_string(&config_path)?;
        toml::from_str(&existing_str).unwrap_or(toml::Value::Table(toml::map::Map::new()))
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    // Convert config to toml::Value
    let config_toml = toml::to_string(config)
        .context("Failed to serialize config")?;
    let config_value: toml::Value = toml::from_str(&config_toml)?;

    // Merge configs (new config takes precedence)
    let mut merged = existing_toml;
    if let (toml::Value::Table(ref mut merged_table), toml::Value::Table(config_table)) = (&mut merged, config_value) {
        for (key, value) in config_table {
            merged_table.insert(key, value);
        }
    }

    // Write merged config
    let merged_str = toml::to_string_pretty(&merged)
        .context("Failed to serialize merged config")?;

    fs::write(&config_path, merged_str)
        .context("Failed to write config.toml")?;

    Ok(())
}

/// Update only the Python venv strategy preference
pub fn set_venv_strategy(project_root: &Path, strategy: VenvStrategy, venv_path: Option<String>) -> Result<()> {
    let mut config = load_config(project_root)?;
    config.python.venv_strategy = strategy;
    config.python.venv_path = venv_path;
    save_config(project_root, &config)?;
    Ok(())
}
