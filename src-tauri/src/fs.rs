use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

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

/// Create a new workbook file (.ipynb format)
pub fn create_workbook(workbook_path: &Path, workbook_name: &str) -> Result<String> {
    // Ensure the parent directory exists
    if !workbook_path.exists() {
        fs::create_dir_all(workbook_path)
            .context("Failed to create workbook directory")?;
    }

    // Ensure the path ends with .ipynb
    let file_path = if workbook_name.ends_with(".ipynb") {
        workbook_path.join(workbook_name)
    } else {
        workbook_path.join(format!("{}.ipynb", workbook_name))
    };

    // Check if file already exists
    if file_path.exists() {
        anyhow::bail!("Workbook already exists: {}", file_path.display());
    }

    // Create basic Jupyter notebook structure
    let notebook_content = json!({
        "cells": [],
        "metadata": {
            "label": workbook_name.trim_end_matches(".ipynb"),
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
        .context("Failed to serialize workbook")?;

    fs::write(&file_path, content)
        .context("Failed to write workbook file")?;

    Ok(file_path.to_string_lossy().to_string())
}

/// Read a workbook file
pub fn read_workbook(workbook_path: &Path) -> Result<String> {
    let content = fs::read_to_string(workbook_path)
        .context("Failed to read workbook file")?;

    Ok(content)
}

/// Configuration for output truncation
const MAX_OUTPUT_LINES: usize = 1000;
const MAX_OUTPUT_CHARS: usize = 100_000; // 100KB per output

/// Truncate a text output if it exceeds limits
fn truncate_output_text(text: &str, max_lines: usize, max_chars: usize) -> (String, bool) {
    let mut truncated = false;
    let mut result = text.to_string();

    // First check character limit
    if result.len() > max_chars {
        result = result.chars().take(max_chars).collect();
        result.push_str("\n... [output truncated, exceeded character limit] ...");
        truncated = true;
    }

    // Then check line limit
    let lines: Vec<&str> = result.lines().collect();
    if lines.len() > max_lines {
        result = lines.iter()
            .take(max_lines)
            .cloned()
            .collect::<Vec<&str>>()
            .join("\n");
        result.push_str("\n... [output truncated, exceeded line limit] ...");
        truncated = true;
    }

    (result, truncated)
}

/// Save workbook content with output truncation to reduce memory usage
pub fn save_workbook(workbook_path: &Path, content: &str) -> Result<()> {
    // Validate and parse JSON before saving
    let mut notebook: serde_json::Value = serde_json::from_str(content)
        .context("Invalid workbook JSON")?;

    // Truncate large outputs to prevent memory issues
    if let Some(cells) = notebook["cells"].as_array_mut() {
        for cell in cells.iter_mut() {
            if cell["cell_type"] == "code" {
                if let Some(outputs) = cell["outputs"].as_array_mut() {
                    for output in outputs.iter_mut() {
                        match output["output_type"].as_str() {
                            Some("stream") => {
                                // Truncate stream output (stdout/stderr)
                                if let Some(text) = output["text"].as_str() {
                                    let (truncated_text, was_truncated) = truncate_output_text(
                                        text,
                                        MAX_OUTPUT_LINES,
                                        MAX_OUTPUT_CHARS
                                    );
                                    output["text"] = serde_json::json!(truncated_text);
                                    if was_truncated {
                                        output["metadata"]["truncated"] = serde_json::json!(true);
                                    }
                                } else if let Some(text_array) = output["text"].as_array() {
                                    // Handle text as array of strings
                                    let combined = text_array
                                        .iter()
                                        .filter_map(|v| v.as_str())
                                        .collect::<Vec<&str>>()
                                        .join("");
                                    let (truncated_text, was_truncated) = truncate_output_text(
                                        &combined,
                                        MAX_OUTPUT_LINES,
                                        MAX_OUTPUT_CHARS
                                    );
                                    output["text"] = serde_json::json!(truncated_text);
                                    if was_truncated {
                                        output["metadata"]["truncated"] = serde_json::json!(true);
                                    }
                                }
                            }
                            Some("execute_result") | Some("display_data") => {
                                // Truncate text/plain representation
                                if let Some(data) = output["data"].as_object_mut() {
                                    if let Some(text_plain) = data.get("text/plain").and_then(|v| v.as_str()) {
                                        let (truncated_text, was_truncated) = truncate_output_text(
                                            text_plain,
                                            MAX_OUTPUT_LINES,
                                            MAX_OUTPUT_CHARS
                                        );
                                        data.insert("text/plain".to_string(), serde_json::json!(truncated_text));
                                        if was_truncated {
                                            if output["metadata"].is_null() {
                                                output["metadata"] = serde_json::json!({});
                                            }
                                            output["metadata"]["truncated"] = serde_json::json!(true);
                                        }
                                    }
                                }
                            }
                            Some("error") => {
                                // Truncate error traceback
                                if let Some(traceback) = output["traceback"].as_array_mut() {
                                    let combined = traceback
                                        .iter()
                                        .filter_map(|v| v.as_str())
                                        .collect::<Vec<&str>>()
                                        .join("\n");
                                    let (truncated_text, was_truncated) = truncate_output_text(
                                        &combined,
                                        MAX_OUTPUT_LINES,
                                        MAX_OUTPUT_CHARS
                                    );
                                    // Split back into array
                                    let lines: Vec<String> = truncated_text.lines().map(|s| s.to_string()).collect();
                                    output["traceback"] = serde_json::json!(lines);
                                    if was_truncated {
                                        output["metadata"]["truncated"] = serde_json::json!(true);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    // Serialize with truncated outputs
    let content = serde_json::to_string_pretty(&notebook)
        .context("Failed to serialize workbook")?;

    fs::write(workbook_path, content)
        .context("Failed to write workbook file")?;

    Ok(())
}

/// Read a generic text file
pub fn read_file(file_path: &Path) -> Result<String> {
    let content = fs::read_to_string(file_path)
        .context("Failed to read file")?;

    Ok(content)
}

/// Read a file as binary (for images, etc.)
pub fn read_file_binary(file_path: &Path) -> Result<Vec<u8>> {
    let content = fs::read(file_path)
        .context("Failed to read file as binary")?;

    Ok(content)
}

/// Save generic text file content
pub fn save_file(file_path: &Path, content: &str) -> Result<()> {
    fs::write(file_path, content)
        .context("Failed to write file")?;

    Ok(())
}

/// Rename a file or directory
pub fn rename_file(old_path: &Path, new_name: &str) -> Result<String> {
    let parent = old_path.parent()
        .context("Failed to get parent directory")?;

    let new_path = parent.join(new_name);

    if new_path.exists() {
        anyhow::bail!("A file with that name already exists");
    }

    fs::rename(old_path, &new_path)
        .context("Failed to rename file")?;

    Ok(new_path.to_string_lossy().to_string())
}

/// Delete a file or directory
pub fn delete_file(file_path: &Path) -> Result<()> {
    if !file_path.exists() {
        anyhow::bail!("File does not exist");
    }

    if file_path.is_dir() {
        fs::remove_dir_all(file_path)
            .context("Failed to delete directory")?;
    } else {
        fs::remove_file(file_path)
            .context("Failed to delete file")?;
    }

    Ok(())
}

/// Duplicate a workbook with a new name
pub fn duplicate_workbook(source_path: &Path, new_name: &str) -> Result<String> {
    let parent = source_path.parent()
        .context("Failed to get parent directory")?;

    // Ensure new name has .ipynb extension
    let new_name = if new_name.ends_with(".ipynb") {
        new_name.to_string()
    } else {
        format!("{}.ipynb", new_name)
    };

    let target_path = parent.join(&new_name);

    if target_path.exists() {
        anyhow::bail!("A workbook with that name already exists");
    }

    // Read source workbook
    let content = fs::read_to_string(source_path)
        .context("Failed to read source workbook")?;

    // Parse and clear outputs (makes duplicates cleaner)
    let mut notebook: serde_json::Value = serde_json::from_str(&content)
        .context("Invalid workbook JSON")?;

    if let Some(cells) = notebook["cells"].as_array_mut() {
        for cell in cells {
            if cell["cell_type"] == "code" {
                cell["outputs"] = serde_json::json!([]);
                cell["execution_count"] = serde_json::json!(null);
            }
        }
    }

    // Write to new file
    let content = serde_json::to_string_pretty(&notebook)
        .context("Failed to serialize workbook")?;

    fs::write(&target_path, content)
        .context("Failed to write duplicate workbook")?;

    Ok(target_path.to_string_lossy().to_string())
}

/// Save a dropped file to the appropriate location
/// .ipynb files go to /notebooks, everything else goes to project root
pub fn save_dropped_file(project_root: &Path, file_name: &str, file_content: &[u8]) -> Result<String> {
    // Determine the target directory based on file extension
    let target_dir = if file_name.ends_with(".ipynb") {
        project_root.join("notebooks")
    } else {
        project_root.to_path_buf()
    };

    // Ensure target directory exists
    if !target_dir.exists() {
        fs::create_dir_all(&target_dir)
            .context("Failed to create target directory")?;
    }

    let target_path = target_dir.join(file_name);

    // Check if file already exists
    if target_path.exists() {
        anyhow::bail!("A file with that name already exists");
    }

    // Write the file
    fs::write(&target_path, file_content)
        .context("Failed to write dropped file")?;

    Ok(target_path.to_string_lossy().to_string())
}

/// Recursively copy a folder to the project root
pub fn copy_folder_recursively(source_path: &Path, dest_path: &Path) -> Result<()> {
    // Ensure destination parent exists
    if let Some(parent) = dest_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .context("Failed to create destination parent directory")?;
        }
    }

    // Create the destination folder
    fs::create_dir_all(dest_path)
        .context("Failed to create destination directory")?;

    // Iterate through source directory
    for entry in fs::read_dir(source_path)
        .context("Failed to read source directory")? {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();
        let file_name = entry.file_name();
        let dest_entry_path = dest_path.join(&file_name);

        if path.is_dir() {
            // Recursively copy subdirectory
            copy_folder_recursively(&path, &dest_entry_path)?;
        } else {
            // Copy file
            fs::copy(&path, &dest_entry_path)
                .context("Failed to copy file")?;
        }
    }

    Ok(())
}

/// Save a dropped folder to the project root
pub fn save_dropped_folder(project_root: &Path, folder_path: &Path) -> Result<String> {
    // Extract folder name
    let folder_name = folder_path.file_name()
        .ok_or_else(|| anyhow::anyhow!("Invalid folder path"))?;

    let dest_path = project_root.join(folder_name);

    // Check if folder already exists
    if dest_path.exists() {
        anyhow::bail!("A folder with that name already exists");
    }

    // Recursively copy the folder
    copy_folder_recursively(folder_path, &dest_path)?;

    Ok(dest_path.to_string_lossy().to_string())
}

/// Handle a dropped item (file or folder) - detects type and saves appropriately
/// This function does all filesystem operations in Rust, avoiding frontend ACL restrictions
pub fn handle_dropped_item(project_root: &Path, item_path: &Path) -> Result<String> {
    // Check if the item exists
    if !item_path.exists() {
        anyhow::bail!("Dropped item does not exist: {}", item_path.display());
    }

    // Get metadata to determine if it's a file or directory
    let metadata = fs::metadata(item_path)
        .context("Failed to read item metadata")?;

    if metadata.is_dir() {
        // Handle as folder
        save_dropped_folder(project_root, item_path)
    } else {
        // Handle as file - read it and save using existing function
        let file_name = item_path.file_name()
            .ok_or_else(|| anyhow::anyhow!("Invalid file path"))?
            .to_string_lossy()
            .to_string();

        let file_content = fs::read(item_path)
            .context("Failed to read dropped file")?;

        save_dropped_file(project_root, &file_name, &file_content)
    }
}

/// Create a new empty file
pub fn create_new_file(parent_path: &Path, file_name: &str, initial_content: Option<&str>) -> Result<String> {
    // Ensure parent directory exists
    if !parent_path.exists() {
        anyhow::bail!("Parent directory does not exist");
    }

    let file_path = parent_path.join(file_name);

    // Check if file already exists
    if file_path.exists() {
        anyhow::bail!("A file with that name already exists");
    }

    // Write initial content (empty string if none provided)
    let content = initial_content.unwrap_or("");
    fs::write(&file_path, content)
        .context("Failed to create file")?;

    Ok(file_path.to_string_lossy().to_string())
}

/// Create a new folder
pub fn create_new_folder(parent_path: &Path, folder_name: &str) -> Result<String> {
    // Ensure parent directory exists
    if !parent_path.exists() {
        anyhow::bail!("Parent directory does not exist");
    }

    let folder_path = parent_path.join(folder_name);

    // Check if folder already exists
    if folder_path.exists() {
        anyhow::bail!("A folder with that name already exists");
    }

    // Create the directory
    fs::create_dir(&folder_path)
        .context("Failed to create folder")?;

    Ok(folder_path.to_string_lossy().to_string())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
    pub is_file: bool,
    pub modified: Option<String>,
    pub created: Option<String>,
    pub readonly: bool,
}

/// Get detailed information about a file
pub fn get_file_info(file_path: &Path) -> Result<FileInfo> {
    if !file_path.exists() {
        anyhow::bail!("File does not exist");
    }

    let metadata = fs::metadata(file_path)
        .context("Failed to read file metadata")?;

    let name = file_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let modified = metadata.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs().to_string());

    let created = metadata.created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs().to_string());

    Ok(FileInfo {
        name,
        path: file_path.to_string_lossy().to_string(),
        size: metadata.len(),
        is_dir: metadata.is_dir(),
        is_file: metadata.is_file(),
        modified,
        created,
        readonly: metadata.permissions().readonly(),
    })
}

/// Reveal a file in Finder (macOS only)
#[cfg(target_os = "macos")]
pub fn reveal_in_finder(file_path: &Path) -> Result<()> {
    if !file_path.exists() {
        anyhow::bail!("File does not exist");
    }

    std::process::Command::new("open")
        .arg("-R")
        .arg(file_path)
        .spawn()
        .context("Failed to reveal file in Finder")?;

    Ok(())
}

/// Reveal a file in Explorer (Windows)
#[cfg(target_os = "windows")]
pub fn reveal_in_finder(file_path: &Path) -> Result<()> {
    if !file_path.exists() {
        anyhow::bail!("File does not exist");
    }

    std::process::Command::new("explorer")
        .arg("/select,")
        .arg(file_path)
        .spawn()
        .context("Failed to reveal file in Explorer")?;

    Ok(())
}

/// Reveal a file in file manager (Linux)
#[cfg(target_os = "linux")]
pub fn reveal_in_finder(file_path: &Path) -> Result<()> {
    if !file_path.exists() {
        anyhow::bail!("File does not exist");
    }

    // Try xdg-open on the parent directory
    if let Some(parent) = file_path.parent() {
        std::process::Command::new("xdg-open")
            .arg(parent)
            .spawn()
            .context("Failed to open file location")?;
    }

    Ok(())
}

// ===== Notebook Versioning =====

#[derive(Debug, Serialize, Deserialize)]
pub struct NotebookVersion {
    pub timestamp: i64,
    pub filename: String,
    pub path: String,
}

/// Get the versions directory for a given project root
fn get_versions_dir(project_root: &Path) -> PathBuf {
    project_root.join(".workbooks").join("versions")
}

/// Get the version directory for a specific notebook
fn get_notebook_version_dir(project_root: &Path, workbook_path: &Path) -> Result<PathBuf> {
    // Extract notebook name from path
    let notebook_name = workbook_path
        .file_stem()
        .ok_or_else(|| anyhow::anyhow!("Invalid notebook path"))?
        .to_string_lossy()
        .to_string();

    Ok(get_versions_dir(project_root).join(notebook_name))
}

/// Save the current version of a notebook before modifying it
/// Returns the path to the saved version
pub fn save_notebook_version(project_root: &Path, workbook_path: &Path) -> Result<String> {
    // Only save if the notebook currently exists
    if !workbook_path.exists() {
        return Ok(String::new()); // No current version to save
    }

    // Create versions directory structure
    let version_dir = get_notebook_version_dir(project_root, workbook_path)?;
    if !version_dir.exists() {
        fs::create_dir_all(&version_dir)
            .context("Failed to create versions directory")?;
    }

    // Generate timestamp-based filename
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let version_filename = format!("{}.ipynb", timestamp);
    let version_path = version_dir.join(&version_filename);

    // Copy current notebook to versions directory
    fs::copy(workbook_path, &version_path)
        .context("Failed to save notebook version")?;

    Ok(version_path.to_string_lossy().to_string())
}

/// List all versions of a notebook
pub fn list_notebook_versions(project_root: &Path, workbook_path: &Path) -> Result<Vec<NotebookVersion>> {
    let version_dir = get_notebook_version_dir(project_root, workbook_path)?;

    if !version_dir.exists() {
        return Ok(Vec::new()); // No versions yet
    }

    let mut versions = Vec::new();

    for entry in fs::read_dir(&version_dir)
        .context("Failed to read versions directory")? {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) == Some("ipynb") {
            if let Some(filename) = path.file_stem() {
                if let Ok(timestamp) = filename.to_string_lossy().parse::<i64>() {
                    versions.push(NotebookVersion {
                        timestamp,
                        filename: entry.file_name().to_string_lossy().to_string(),
                        path: path.to_string_lossy().to_string(),
                    });
                }
            }
        }
    }

    // Sort by timestamp descending (newest first)
    versions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(versions)
}

/// Get the content of a specific notebook version
pub fn get_notebook_version(project_root: &Path, workbook_path: &Path, timestamp: i64) -> Result<String> {
    let version_dir = get_notebook_version_dir(project_root, workbook_path)?;
    let version_filename = format!("{}.ipynb", timestamp);
    let version_path = version_dir.join(version_filename);

    if !version_path.exists() {
        anyhow::bail!("Version not found");
    }

    read_file(&version_path)
}

/// Get the previous (most recent) version of a notebook
pub fn get_previous_notebook_version(project_root: &Path, workbook_path: &Path) -> Result<Option<String>> {
    let versions = list_notebook_versions(project_root, workbook_path)?;

    if versions.is_empty() {
        return Ok(None);
    }

    // Get the most recent version (already sorted newest first)
    let latest = &versions[0];
    let content = get_notebook_version(project_root, workbook_path, latest.timestamp)?;

    Ok(Some(content))
}

/// Revert a notebook to a specific version
pub fn revert_notebook_to_version(project_root: &Path, workbook_path: &Path, timestamp: i64) -> Result<()> {
    // First, save the current state as a version (before reverting)
    save_notebook_version(project_root, workbook_path)?;

    // Get the version content
    let version_content = get_notebook_version(project_root, workbook_path, timestamp)?;

    // Write it to the workbook path
    save_workbook(workbook_path, &version_content)?;

    Ok(())
}

/// Delete old notebook versions, keeping only the most recent N versions
pub fn cleanup_old_versions(project_root: &Path, workbook_path: &Path, keep_count: usize) -> Result<usize> {
    let mut versions = list_notebook_versions(project_root, workbook_path)?;

    if versions.len() <= keep_count {
        return Ok(0); // Nothing to delete
    }

    // Sort by timestamp ascending (oldest first) for deletion
    versions.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    let to_delete = versions.len() - keep_count;
    let mut deleted = 0;

    for version in versions.iter().take(to_delete) {
        if let Ok(_) = fs::remove_file(&version.path) {
            deleted += 1;
        }
    }

    Ok(deleted)
}
