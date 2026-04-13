use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::executor::BlockResult;

#[derive(Serialize, Deserialize)]
pub struct Checkpoint {
    pub version: u32,
    pub workbook: String,
    pub status: CheckpointStatus,
    pub next_block: usize,
    pub total_blocks: usize,
    pub started_at: String,
    pub updated_at: String,
    pub results: Vec<SavedResult>,
}

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointStatus {
    InProgress,
    Complete,
    Failed,
}

#[derive(Serialize, Deserialize)]
pub struct SavedResult {
    pub block_index: usize,
    pub language: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    #[serde(default)]
    pub line_number: usize,
    #[serde(default)]
    pub heading: Option<String>,
    #[serde(default)]
    pub code_hash: Option<String>,
}

impl Checkpoint {
    pub fn new(workbook: &str, total_blocks: usize) -> Self {
        let now = Utc::now().to_rfc3339();
        Checkpoint {
            version: 1,
            workbook: workbook.to_string(),
            status: CheckpointStatus::InProgress,
            next_block: 0,
            total_blocks,
            started_at: now.clone(),
            updated_at: now,
            results: Vec::new(),
        }
    }

    pub fn add_result(&mut self, result: &BlockResult, line_number: usize, heading: Option<&str>, code: &str) {
        self.results.push(SavedResult {
            block_index: result.block_index,
            language: result.language.clone(),
            stdout: result.stdout.clone(),
            stderr: result.stderr.clone(),
            exit_code: result.exit_code,
            duration_ms: result.duration.as_millis() as u64,
            line_number,
            heading: heading.map(|s| s.to_string()),
            code_hash: Some(hash_code(code)),
        });
        self.next_block = result.block_index + 1;
        self.updated_at = Utc::now().to_rfc3339();
    }

    pub fn mark_complete(&mut self) {
        self.status = CheckpointStatus::Complete;
        self.updated_at = Utc::now().to_rfc3339();
    }

    pub fn mark_failed(&mut self) {
        self.status = CheckpointStatus::Failed;
        self.updated_at = Utc::now().to_rfc3339();
    }

    /// Convert saved results back to BlockResults for merging into summaries
    pub fn block_results(&self) -> Vec<BlockResult> {
        self.results
            .iter()
            .map(|r| BlockResult {
                block_index: r.block_index,
                language: r.language.clone(),
                stdout: r.stdout.clone(),
                stderr: r.stderr.clone(),
                exit_code: r.exit_code,
                duration: Duration::from_millis(r.duration_ms),
            })
            .collect()
    }
}

pub fn hash_code(code: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(code.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn checkpoint_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    Path::new(&home).join(".wb").join("checkpoints")
}

fn checkpoint_path(id: &str) -> PathBuf {
    checkpoint_dir().join(format!("{}.json", id))
}

pub fn save(id: &str, checkpoint: &Checkpoint) -> Result<(), String> {
    let dir = checkpoint_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create checkpoint dir: {}", e))?;
    let path = checkpoint_path(id);
    let json =
        serde_json::to_string_pretty(checkpoint).map_err(|e| format!("serialize checkpoint: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("write checkpoint: {}", e))?;
    Ok(())
}

pub fn load(id: &str) -> Result<Option<Checkpoint>, String> {
    let path = checkpoint_path(id);
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path).map_err(|e| format!("read checkpoint: {}", e))?;
    let checkpoint: Checkpoint =
        serde_json::from_str(&content).map_err(|e| format!("parse checkpoint: {}", e))?;
    Ok(Some(checkpoint))
}
