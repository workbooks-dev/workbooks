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
