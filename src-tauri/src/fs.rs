use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_file: bool,
    pub extension: Option<String>,
}

/// List files and directories in a given path
pub fn list_directory(directory_path: &Path) -> Result<Vec<FileEntry>> {
    let mut entries = Vec::new();

    let read_dir = fs::read_dir(directory_path)
        .context("Failed to read directory")?;

    for entry in read_dir {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();
        let metadata = entry.metadata().context("Failed to read metadata")?;

        let name = entry
            .file_name()
            .to_string_lossy()
            .to_string();

        // Skip hidden files that start with .
        if name.starts_with('.') {
            continue;
        }

        let extension = if metadata.is_file() {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|s| s.to_string())
        } else {
            None
        };

        entries.push(FileEntry {
            name,
            path: path.to_string_lossy().to_string(),
            is_dir: metadata.is_dir(),
            is_file: metadata.is_file(),
            extension,
        });
    }

    // Sort: directories first, then files, both alphabetically
    entries.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    Ok(entries)
}

/// Create a new Jupyter notebook file
pub fn create_notebook(notebook_path: &Path, notebook_name: &str) -> Result<String> {
    // Ensure the path ends with .ipynb
    let file_path = if notebook_name.ends_with(".ipynb") {
        notebook_path.join(notebook_name)
    } else {
        notebook_path.join(format!("{}.ipynb", notebook_name))
    };

    // Check if file already exists
    if file_path.exists() {
        anyhow::bail!("Notebook already exists: {}", file_path.display());
    }

    // Create basic Jupyter notebook structure
    let notebook_content = json!({
        "cells": [],
        "metadata": {
            "kernelspec": {
                "display_name": "Python 3",
                "language": "python",
                "name": "python3"
            },
            "language_info": {
                "name": "python",
                "version": "3.11.0",
                "mimetype": "text/x-python",
                "codemirror_mode": {
                    "name": "ipython",
                    "version": 3
                },
                "pygments_lexer": "ipython3",
                "nbconvert_exporter": "python",
                "file_extension": ".py"
            }
        },
        "nbformat": 4,
        "nbformat_minor": 5
    });

    // Write to file
    let content = serde_json::to_string_pretty(&notebook_content)
        .context("Failed to serialize notebook")?;

    fs::write(&file_path, content)
        .context("Failed to write notebook file")?;

    Ok(file_path.to_string_lossy().to_string())
}

/// Read a notebook file
pub fn read_notebook(notebook_path: &Path) -> Result<String> {
    let content = fs::read_to_string(notebook_path)
        .context("Failed to read notebook file")?;

    Ok(content)
}

/// Save notebook content
pub fn save_notebook(notebook_path: &Path, content: &str) -> Result<()> {
    // Validate JSON before saving
    let _: serde_json::Value = serde_json::from_str(content)
        .context("Invalid notebook JSON")?;

    fs::write(notebook_path, content)
        .context("Failed to write notebook file")?;

    Ok(())
}

/// Read a generic text file
pub fn read_file(file_path: &Path) -> Result<String> {
    let content = fs::read_to_string(file_path)
        .context("Failed to read file")?;

    Ok(content)
}

/// Save generic text file content
pub fn save_file(file_path: &Path, content: &str) -> Result<()> {
    fs::write(file_path, content)
        .context("Failed to write file")?;

    Ok(())
}
