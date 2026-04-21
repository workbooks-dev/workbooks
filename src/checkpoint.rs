use std::collections::HashMap;
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
    /// Variables populated by `wait` signal resumes, merged into workbook vars on replay.
    #[serde(default)]
    pub bound_vars: HashMap<String, String>,
    /// Section indices of `wait` blocks that have already been satisfied.
    #[serde(default)]
    pub waits_completed: Vec<usize>,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointStatus {
    InProgress,
    Complete,
    Failed,
    Paused,
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
            bound_vars: HashMap::new(),
            waits_completed: Vec::new(),
        }
    }

    pub fn mark_paused(&mut self) {
        self.status = CheckpointStatus::Paused;
        self.updated_at = Utc::now().to_rfc3339();
    }

    pub fn mark_in_progress(&mut self) {
        self.status = CheckpointStatus::InProgress;
        self.updated_at = Utc::now().to_rfc3339();
    }

    pub fn complete_wait(&mut self, section_index: usize) {
        if !self.waits_completed.contains(&section_index) {
            self.waits_completed.push(section_index);
        }
        self.updated_at = Utc::now().to_rfc3339();
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
                error_type: None,
            })
            .collect()
    }
}

pub fn hash_code(code: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(code.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn checkpoint_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    Path::new(&home).join(".wb").join("checkpoints")
}

pub fn checkpoint_path(id: &str) -> PathBuf {
    checkpoint_dir().join(format!("{}.json", id))
}

pub fn delete(id: &str) -> Result<(), String> {
    let path = checkpoint_path(id);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("remove checkpoint: {}", e))?;
    }
    Ok(())
}

pub fn save(id: &str, checkpoint: &Checkpoint) -> Result<(), String> {
    let dir = checkpoint_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create checkpoint dir: {}", e))?;
    let path = checkpoint_path(id);
    let json =
        serde_json::to_string_pretty(checkpoint).map_err(|e| format!("serialize checkpoint: {}", e))?;
    crate::atomic_io::write_secret_file(&path, json.as_bytes())
        .map_err(|e| format!("write checkpoint: {}", e))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_id(prefix: &str) -> String {
        let n = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("{}_{}_{}_{}", prefix, std::process::id(), ts, n)
    }

    #[test]
    fn test_mark_paused_and_in_progress() {
        let mut ckpt = Checkpoint::new("test.md", 5);
        assert_eq!(ckpt.status, CheckpointStatus::InProgress);

        ckpt.mark_paused();
        assert_eq!(ckpt.status, CheckpointStatus::Paused);

        let paused_at = ckpt.updated_at.clone();

        // Tiny sleep so timestamps differ
        std::thread::sleep(Duration::from_millis(10));

        ckpt.mark_in_progress();
        assert_eq!(ckpt.status, CheckpointStatus::InProgress);
        assert_ne!(ckpt.updated_at, paused_at, "updated_at should change");
    }

    #[test]
    fn test_complete_wait() {
        let mut ckpt = Checkpoint::new("test.md", 3);
        assert!(ckpt.waits_completed.is_empty());

        ckpt.complete_wait(2);
        assert_eq!(ckpt.waits_completed, vec![2]);

        // Completing the same section_index again should not duplicate
        ckpt.complete_wait(2);
        assert_eq!(ckpt.waits_completed, vec![2]);

        ckpt.complete_wait(5);
        assert_eq!(ckpt.waits_completed, vec![2, 5]);
    }

    #[test]
    fn test_bound_vars_persist_through_save_load() {
        let id = unique_id("test_ckpt_bound_vars");
        let mut ckpt = Checkpoint::new("test.md", 3);
        ckpt.bound_vars.insert("otp_code".to_string(), "123456".to_string());
        ckpt.bound_vars.insert("sender".to_string(), "auth@example.com".to_string());
        ckpt.waits_completed.push(1);
        ckpt.waits_completed.push(4);
        ckpt.mark_paused();

        save(&id, &ckpt).expect("save should succeed");
        let loaded = load(&id).expect("load should not error").expect("should find checkpoint");

        assert_eq!(loaded.bound_vars.get("otp_code").unwrap(), "123456");
        assert_eq!(loaded.bound_vars.get("sender").unwrap(), "auth@example.com");
        assert_eq!(loaded.bound_vars.len(), 2);
        assert_eq!(loaded.waits_completed, vec![1, 4]);
        assert_eq!(loaded.status, CheckpointStatus::Paused);
        assert_eq!(loaded.workbook, "test.md");
        assert_eq!(loaded.total_blocks, 3);

        // Clean up
        delete(&id).expect("cleanup");
    }

    #[test]
    fn test_save_load_delete_roundtrip() {
        let id = unique_id("test_ckpt_roundtrip");
        let ckpt = Checkpoint::new("deploy.md", 5);

        save(&id, &ckpt).expect("save");
        assert!(load(&id).expect("load").is_some());

        delete(&id).expect("delete");
        assert!(load(&id).expect("load after delete").is_none());
    }

    #[test]
    fn test_load_nonexistent_returns_none() {
        let result = load("test_ckpt_nonexistent_999999").expect("load should not error");
        assert!(result.is_none());
    }

    #[test]
    fn test_mark_complete_and_failed() {
        let mut ckpt = Checkpoint::new("test.md", 2);

        ckpt.mark_complete();
        assert_eq!(ckpt.status, CheckpointStatus::Complete);

        ckpt.mark_failed();
        assert_eq!(ckpt.status, CheckpointStatus::Failed);
    }

    #[test]
    fn test_checkpoint_new_defaults() {
        let ckpt = Checkpoint::new("my-workbook.md", 10);
        assert_eq!(ckpt.version, 1);
        assert_eq!(ckpt.workbook, "my-workbook.md");
        assert_eq!(ckpt.status, CheckpointStatus::InProgress);
        assert_eq!(ckpt.next_block, 0);
        assert_eq!(ckpt.total_blocks, 10);
        assert!(ckpt.results.is_empty());
        assert!(ckpt.bound_vars.is_empty());
        assert!(ckpt.waits_completed.is_empty());
        // started_at and updated_at should be valid rfc3339
        chrono::DateTime::parse_from_rfc3339(&ckpt.started_at)
            .expect("started_at should be valid rfc3339");
        chrono::DateTime::parse_from_rfc3339(&ckpt.updated_at)
            .expect("updated_at should be valid rfc3339");
    }
}
