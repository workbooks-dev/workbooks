use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use fs4::fs_std::FileExt;
use serde::{Deserialize, Serialize};

use crate::checkpoint;
use crate::parser::{self, BindSpec, WaitSpec};

/// Pending-signal descriptor written next to a paused checkpoint.
/// External resolvers read these, watch whatever source `kind` names,
/// and invoke `wb resume <id> --signal <payload>` when a match arrives.
#[derive(Serialize, Deserialize, Debug)]
pub struct PendingDescriptor {
    pub checkpoint: String,
    pub checkpoint_id: String,
    pub workbook: String,
    /// 1-indexed code-block position this wait follows (for humans).
    pub next_block: usize,
    /// Line number of the `wait` (or `browser`) fence in the source markdown.
    pub line_number: usize,
    pub section_index: usize,
    pub kind: Option<String>,
    #[serde(rename = "match", skip_serializing_if = "Option::is_none")]
    pub match_: Option<serde_yaml::Value>,
    pub bind: Option<BindSpec>,
    pub created_at: String,
    pub timeout_at: Option<String>,
    pub on_timeout: Option<String>,
    /// Opaque sidecar state captured at pause, restored on resume. Populated
    /// only for browser-slice pauses; `wb` does not interpret it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidecar_state: Option<serde_yaml::Value>,
    /// Browserbase live-view URL (or equivalent) the human clicks to resolve
    /// a slice-internal pause (MFA, OTP).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_url: Option<String>,
    /// Verb position within the paused slice. Surfaces in `wb pending`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verb_index: Option<usize>,
    /// Operator-facing prompt from `pause_for_human`. Rendered on the run
    /// page so an operator seeing `wb pending` output or a dashboard knows
    /// what they're being asked to do without reading the markdown source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Deep-link for the off-band action (Drive folder, approval console,
    /// MFA challenge URL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_url: Option<String>,
    /// One of "operator_click" | "poll" | "timeout". Run page uses this
    /// to pick the right auto-resume behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_on: Option<String>,
    /// Pre-parsed duration string (for display). `timeout_at` is the
    /// authoritative wall-clock deadline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    /// Operator button set. Empty / missing → single default "Resume"
    /// button. A non-empty list enables branching: the chosen value lands
    /// in `$WB_ARTIFACTS_DIR/pause_result.json` at resume time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<serde_json::Value>,
    /// Callback URL the original `wb run` was invoked with (`--callback`).
    /// Persisted so timeout reaping can fire `checkpoint.failed` against the
    /// same endpoint that received the original `workbook.paused` event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_url: Option<String>,
    /// HMAC secret paired with `callback_url`. Same persistence reasoning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_secret: Option<String>,
}

pub fn descriptor_path(id: &str) -> PathBuf {
    checkpoint::checkpoint_dir().join(format!("{}.pending.json", id))
}

/// Path to the per-id sidecar lock file used to serialize reap operations.
/// We lock a sibling `.lock` file rather than the descriptor itself because the
/// descriptor is deleted as part of reaping — locking a file you're about to
/// remove is fragile (the lock fd is still held but the inode is gone, and
/// readers via `descriptor_path` race the unlink).
fn reap_lock_path(id: &str) -> PathBuf {
    checkpoint::checkpoint_dir().join(format!("{}.pending.reap.lock", id))
}

/// Acquire a per-id exclusive advisory lock on a sidecar `.lock` file, run the
/// closure, then release the lock (via Drop). Concurrent reapers serialize on
/// the same id; reapers for different ids don't contend.
///
/// The lock file is left in place across calls — it's a tiny zero-byte file
/// and creating/removing it on every reap would re-introduce races. It lives
/// next to the checkpoint state in `~/.wb/checkpoints/`.
fn with_pending_lock<F, T>(id: &str, f: F) -> std::io::Result<T>
where
    F: FnOnce() -> T,
{
    let dir = checkpoint::checkpoint_dir();
    std::fs::create_dir_all(&dir)?;
    let path = reap_lock_path(id);
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    FileExt::lock_exclusive(&file)?;
    let result = f();
    // Explicit unlock; Drop on the File would also release, but being explicit
    // makes the lifetime obvious.
    let _ = FileExt::unlock(&file);
    Ok(result)
}

pub fn save(id: &str, desc: &PendingDescriptor) -> Result<(), String> {
    let dir = checkpoint::checkpoint_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create checkpoint dir: {}", e))?;
    let path = descriptor_path(id);
    let json =
        serde_json::to_string_pretty(desc).map_err(|e| format!("serialize descriptor: {}", e))?;
    crate::atomic_io::write_secret_file(&path, json.as_bytes())
        .map_err(|e| format!("write descriptor: {}", e))?;
    Ok(())
}

pub fn load(id: &str) -> Result<Option<PendingDescriptor>, String> {
    let path = descriptor_path(id);
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path).map_err(|e| format!("read descriptor: {}", e))?;
    let desc: PendingDescriptor =
        serde_json::from_str(&content).map_err(|e| format!("parse descriptor: {}", e))?;
    Ok(Some(desc))
}

pub fn delete(id: &str) -> Result<(), String> {
    let path = descriptor_path(id);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("remove descriptor: {}", e))?;
    }
    Ok(())
}

/// List all `*.pending.json` descriptors in the checkpoint dir.
pub fn list_all() -> Vec<(String, PendingDescriptor)> {
    let dir = checkpoint::checkpoint_dir();
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if let Some(id) = name.strip_suffix(".pending.json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(desc) = serde_json::from_str::<PendingDescriptor>(&content) {
                        out.push((id.to_string(), desc));
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

pub fn build(
    checkpoint_id: &str,
    workbook: &str,
    next_block: usize,
    spec: &WaitSpec,
    callback: Option<(&str, Option<&str>)>,
) -> PendingDescriptor {
    let now = Utc::now();
    let timeout_at = spec.timeout.as_deref().and_then(|t| {
        parser::parse_duration_secs(t)
            .ok()
            .map(|secs| (now + ChronoDuration::seconds(secs as i64)).to_rfc3339())
    });
    let ckpt_path = checkpoint::checkpoint_path(checkpoint_id)
        .to_string_lossy()
        .to_string();
    let (callback_url, callback_secret) = match callback {
        Some((url, secret)) => (Some(url.to_string()), secret.map(String::from)),
        None => (None, None),
    };
    PendingDescriptor {
        checkpoint: ckpt_path,
        checkpoint_id: checkpoint_id.to_string(),
        workbook: workbook.to_string(),
        next_block,
        line_number: spec.line_number,
        section_index: spec.section_index,
        kind: spec.kind.clone(),
        match_: spec.match_.clone(),
        bind: spec.bind.clone(),
        created_at: now.to_rfc3339(),
        timeout_at,
        on_timeout: spec.on_timeout.clone(),
        sidecar_state: None,
        resume_url: None,
        verb_index: None,
        message: None,
        context_url: None,
        resume_on: None,
        timeout: None,
        actions: Vec::new(),
        callback_url,
        callback_secret,
    }
}

/// Build a pending descriptor for a browser-slice pause. Sidecar state is
/// opaque — `wb` just persists it so the resumed sidecar can pick up where it
/// left off. The operator-facing fields (message, context_url, resume_on,
/// timeout, actions) come verbatim from the sidecar's `slice.paused` frame
/// via `PauseInfo`; wb doesn't interpret them beyond persistence +
/// forwarding on callbacks.
pub fn build_for_browser_pause(
    checkpoint_id: &str,
    workbook: &str,
    next_block: usize,
    slice: &crate::parser::BrowserSliceSpec,
    pause: &crate::sidecar::PauseInfo,
    callback: Option<(&str, Option<&str>)>,
) -> PendingDescriptor {
    let now = Utc::now();
    let ckpt_path = checkpoint::checkpoint_path(checkpoint_id)
        .to_string_lossy()
        .to_string();
    // If the verb supplied a `timeout:` string we parse it here so the
    // reaper's `timeout_at` comparison is a plain ISO-8601 compare.
    let timeout_at = pause.timeout.as_deref().and_then(|t| {
        parser::parse_duration_secs(t)
            .ok()
            .map(|secs| (now + ChronoDuration::seconds(secs as i64)).to_rfc3339())
    });
    // `resume_on: timeout` → auto-abort on expiry (reaper picks it up).
    // `resume_on: operator_click` / `poll` → leave on_timeout unset so the
    // reaper doesn't touch it.
    let on_timeout = match pause.resume_on.as_deref() {
        Some("timeout") => Some("abort".to_string()),
        _ => None,
    };
    let (callback_url, callback_secret) = match callback {
        Some((url, secret)) => (Some(url.to_string()), secret.map(String::from)),
        None => (None, None),
    };
    PendingDescriptor {
        checkpoint: ckpt_path,
        checkpoint_id: checkpoint_id.to_string(),
        workbook: workbook.to_string(),
        next_block,
        line_number: slice.line_number,
        section_index: slice.section_index,
        kind: pause
            .reason
            .clone()
            .or_else(|| Some("browser.slice_paused".to_string())),
        match_: None,
        bind: None,
        created_at: now.to_rfc3339(),
        timeout_at,
        on_timeout,
        sidecar_state: pause.sidecar_state.clone(),
        resume_url: pause.resume_url.clone(),
        verb_index: pause.verb_index,
        message: pause.message.clone(),
        context_url: pause.context_url.clone(),
        resume_on: pause.resume_on.clone(),
        timeout: pause.timeout.clone(),
        actions: pause.actions.clone(),
        callback_url,
        callback_secret,
    }
}

/// Entry describing one descriptor that was reaped by `reap_expired`.
#[derive(Debug, Clone, Serialize)]
pub struct ReapedEntry {
    pub id: String,
    pub workbook: String,
    pub kind: Option<String>,
    pub on_timeout: Option<String>,
    pub timeout_at: Option<String>,
    /// Whether the checkpoint file was found and marked failed.
    pub checkpoint_marked_failed: bool,
}

/// Reap expired pending descriptors whose `on_timeout` resolves to "abort"
/// semantics (explicit `abort`, unset, or an unrecognised value — which the
/// resume path also treats as abort). For each reaped descriptor:
///   - mark the associated checkpoint as failed (if present)
///   - delete the pending descriptor file
///
/// Modes that need to resume execution (`skip`, `prompt`) are skipped — they
/// require `wb resume` to bind empty values and continue running blocks, which
/// this function deliberately does not do. Best-effort: any I/O error on an
/// individual descriptor is ignored so one broken entry can't stall the sweep.
pub fn reap_expired() -> Vec<ReapedEntry> {
    let mut reaped = Vec::new();
    for (id, desc) in list_all() {
        if !is_expired(&desc) {
            continue;
        }
        let mode = desc.on_timeout.as_deref().unwrap_or("abort");
        if mode == "skip" || mode == "prompt" {
            continue;
        }

        // Take a per-id exclusive lock so two concurrent `wb pending` runs
        // (cron + manual, two CI processes, two threads) don't both load,
        // mark, and delete the same descriptor. The race window is short but
        // the cost of double-reaping is real: spurious ReapedEntry rows in
        // both callers' output, two `mark_failed` saves, two delete syscalls,
        // and a ckpt re-save after another writer that may have rerun the
        // checkpoint into a non-terminal state.
        let lock_outcome = with_pending_lock(&id, || -> Option<ReapedEntry> {
            // Re-check inside the lock — another reaper may have just
            // finished. If the descriptor file is gone, skip silently.
            let desc_now = match load(&id) {
                Ok(Some(d)) => d,
                _ => return None,
            };
            // Another reaper might also have raced an in-place edit; re-honor
            // the same expiry/mode filters under the lock.
            if !is_expired(&desc_now) {
                return None;
            }
            let mode_now = desc_now.on_timeout.as_deref().unwrap_or("abort");
            if mode_now == "skip" || mode_now == "prompt" {
                return None;
            }

            let mut marked = false;
            let mut total_blocks: Option<usize> = None;
            // `we_transitioned` distinguishes "we just moved the checkpoint
            // into the failed state" from "checkpoint was already terminal" /
            // "no paired checkpoint". The original-run failure path would
            // have already emitted checkpoint.failed for an already-terminal
            // checkpoint, so we suppress the reaper's emission in that case
            // to avoid duplicates. When there's no checkpoint at all, the
            // operator never got a callback for this run, so we DO emit.
            let mut we_transitioned = false;
            match checkpoint::load(&id) {
                Ok(Some(mut ckpt)) => {
                    // If a concurrent reaper already marked it failed (or a
                    // resume already ran it to completion), don't clobber the
                    // terminal state.
                    use checkpoint::CheckpointStatus;
                    total_blocks = Some(ckpt.total_blocks);
                    if matches!(
                        ckpt.status,
                        CheckpointStatus::Failed | CheckpointStatus::Complete
                    ) {
                        marked = ckpt.status == CheckpointStatus::Failed;
                    } else {
                        ckpt.mark_failed();
                        if checkpoint::save(&id, &ckpt).is_ok() {
                            marked = true;
                            we_transitioned = true;
                        }
                    }
                }
                _ => {
                    // No paired checkpoint — fine, just delete the
                    // descriptor. Still fire the callback below since the
                    // original run never got a chance to.
                    we_transitioned = true;
                }
            }

            // Best-effort: if the original run was started with --callback,
            // fire `checkpoint.failed` so downstream agents see the timeout
            // the same way they'd see a bail-on-failure event. Failures here
            // are non-fatal — we still delete the descriptor below.
            if we_transitioned {
                if let Some(url) = desc_now.callback_url.as_deref() {
                    let cb = crate::callback::CallbackConfig {
                        url: url.to_string(),
                        secret: desc_now.callback_secret.clone(),
                        stream_key: "wb:events".to_string(),
                        run_id: id.clone(),
                        seq: std::sync::atomic::AtomicU64::new(0),
                    };
                    let total = total_blocks.unwrap_or(desc_now.next_block);
                    let completed = desc_now.next_block.saturating_sub(1);
                    let result = crate::executor::BlockResult {
                        block_index: desc_now.next_block,
                        language: desc_now.kind.clone().unwrap_or_else(|| "wait".to_string()),
                        stdout: String::new(),
                        stderr: format!(
                            "wait timed out (timeout_at={})",
                            desc_now.timeout_at.as_deref().unwrap_or("-")
                        ),
                        exit_code: 124,
                        duration: std::time::Duration::from_secs(0),
                        error_type: Some("timeout".to_string()),
                        stdout_partial: false,
                        stderr_partial: false,
                    };
                    cb.checkpoint_failed(
                        &result,
                        completed,
                        total,
                        &desc_now.workbook,
                        &id,
                        None,
                        desc_now.line_number,
                        &[],
                        // Reap path doesn't have access to the original
                        // workbook's step list — descriptors don't persist
                        // step ids today. Leaving None until we either
                        // persist the id on the descriptor or rebuild the
                        // step list from the workbook file at reap time.
                        None,
                        None,
                    );
                }
            }

            let _ = delete(&id);

            Some(ReapedEntry {
                id: id.clone(),
                workbook: desc_now.workbook.clone(),
                kind: desc_now.kind.clone(),
                on_timeout: desc_now.on_timeout.clone(),
                timeout_at: desc_now.timeout_at.clone(),
                checkpoint_marked_failed: marked,
            })
        });

        if let Ok(Some(entry)) = lock_outcome {
            reaped.push(entry);
        }
        // If lock acquisition failed (rare: I/O error opening the lock file),
        // we silently skip this id. The reap is best-effort and a future
        // `wb pending` invocation will retry.
    }
    reaped
}

/// Returns true if the descriptor's timeout has passed.
pub fn is_expired(desc: &PendingDescriptor) -> bool {
    match desc.timeout_at.as_deref() {
        Some(t) => match DateTime::parse_from_rfc3339(t) {
            Ok(expires) => Utc::now() >= expires.with_timezone(&Utc),
            Err(_) => false,
        },
        None => false,
    }
}

/// Best-effort summary line for `wb pending` output.
pub fn summarize(id: &str, desc: &PendingDescriptor) -> String {
    let kind = desc.kind.as_deref().unwrap_or("-");
    let binds = desc
        .bind
        .as_ref()
        .map(|b| match b {
            BindSpec::Single(s) => s.clone(),
            BindSpec::Multiple(v) => v.join(","),
        })
        .unwrap_or_else(|| "-".to_string());
    let expires = desc.timeout_at.as_deref().unwrap_or("never");
    let expired = if is_expired(desc) { " [EXPIRED]" } else { "" };
    format!(
        "{}  {}  bind={}  expires={}  {}  L{}{}",
        id,
        kind,
        binds,
        expires,
        workbook_basename(&desc.workbook),
        desc.line_number,
        expired,
    )
}

fn workbook_basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{BindSpec, WaitSpec};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Generate a unique test ID to avoid collisions between parallel tests.
    fn unique_id(prefix: &str) -> String {
        let n = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("{}_{}_{}_{}", prefix, std::process::id(), ts, n)
    }

    struct CheckpointDirGuard(Option<PathBuf>);

    impl Drop for CheckpointDirGuard {
        fn drop(&mut self) {
            checkpoint::set_test_checkpoint_dir(self.0.take());
        }
    }

    fn set_thread_checkpoint_dir(dir: PathBuf) -> CheckpointDirGuard {
        CheckpointDirGuard(checkpoint::set_test_checkpoint_dir(Some(dir)))
    }

    fn make_wait_spec() -> WaitSpec {
        WaitSpec {
            kind: Some("email".to_string()),
            match_: Some(serde_yaml::Value::Mapping({
                let mut m = serde_yaml::Mapping::new();
                m.insert(
                    serde_yaml::Value::String("from".to_string()),
                    serde_yaml::Value::String("auth@example.com".to_string()),
                );
                m
            })),
            bind: Some(BindSpec::Single("otp_code".to_string())),
            timeout: Some("5m".to_string()),
            on_timeout: Some("abort".to_string()),
            line_number: 42,
            section_index: 3,
            attrs: Default::default(),
        }
    }

    #[test]
    fn test_build_creates_correct_descriptor() {
        let spec = make_wait_spec();
        let desc = build("ckpt-1", "deploy.md", 2, &spec, None);

        assert_eq!(desc.checkpoint_id, "ckpt-1");
        assert_eq!(desc.workbook, "deploy.md");
        assert_eq!(desc.next_block, 2);
        assert_eq!(desc.line_number, 42);
        assert_eq!(desc.section_index, 3);
        assert_eq!(desc.kind.as_deref(), Some("email"));
        assert_eq!(desc.on_timeout.as_deref(), Some("abort"));

        // bind should carry through
        match &desc.bind {
            Some(BindSpec::Single(s)) => assert_eq!(s, "otp_code"),
            _ => panic!("expected Single bind"),
        }

        // match_ should carry through
        assert!(desc.match_.is_some());

        // timeout_at should be set (5m from now)
        assert!(desc.timeout_at.is_some());
        let expires = chrono::DateTime::parse_from_rfc3339(desc.timeout_at.as_ref().unwrap())
            .expect("timeout_at should be valid rfc3339");
        let now = Utc::now();
        // Should be roughly 5 minutes from now (allow 10s tolerance)
        let diff = expires.with_timezone(&Utc) - now;
        assert!(diff.num_seconds() > 280 && diff.num_seconds() <= 300);

        // created_at should be set
        chrono::DateTime::parse_from_rfc3339(&desc.created_at)
            .expect("created_at should be valid rfc3339");

        // checkpoint path should contain the id
        assert!(desc.checkpoint.contains("ckpt-1"));
    }

    #[test]
    fn test_build_no_timeout() {
        let spec = WaitSpec {
            kind: Some("manual".to_string()),
            timeout: None,
            on_timeout: None,
            bind: None,
            match_: None,
            line_number: 10,
            section_index: 1,
            attrs: Default::default(),
        };
        let desc = build("ckpt-2", "manual.md", 1, &spec, None);
        assert!(desc.timeout_at.is_none());
        assert!(desc.on_timeout.is_none());
        assert!(desc.bind.is_none());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let id = unique_id("test_pending_roundtrip");
        let spec = make_wait_spec();
        let desc = build(&id, "deploy.md", 2, &spec, None);

        save(&id, &desc).expect("save should succeed");
        let loaded = load(&id)
            .expect("load should not error")
            .expect("should find descriptor");

        assert_eq!(loaded.checkpoint_id, desc.checkpoint_id);
        assert_eq!(loaded.workbook, desc.workbook);
        assert_eq!(loaded.next_block, desc.next_block);
        assert_eq!(loaded.line_number, desc.line_number);
        assert_eq!(loaded.section_index, desc.section_index);
        assert_eq!(loaded.kind, desc.kind);
        assert_eq!(loaded.on_timeout, desc.on_timeout);
        assert_eq!(loaded.created_at, desc.created_at);
        assert_eq!(loaded.timeout_at, desc.timeout_at);

        // Clean up
        delete(&id).expect("cleanup delete");
    }

    #[test]
    fn test_load_nonexistent_returns_none() {
        let result = load("test_pending_nonexistent_999999").expect("load should not error");
        assert!(result.is_none());
    }

    #[test]
    fn test_delete_removes_descriptor() {
        let id = unique_id("test_pending_delete");
        let spec = make_wait_spec();
        let desc = build(&id, "deploy.md", 2, &spec, None);

        save(&id, &desc).expect("save should succeed");
        // Confirm it exists
        assert!(load(&id).expect("load").is_some());

        delete(&id).expect("delete should succeed");
        assert!(load(&id).expect("load after delete").is_none());
    }

    #[test]
    fn test_delete_nonexistent_is_ok() {
        // Deleting something that doesn't exist should not error
        delete("test_pending_delete_nonexistent_999999").expect("delete nonexistent should be ok");
    }

    #[test]
    fn test_is_expired_past() {
        let desc = PendingDescriptor {
            checkpoint: "test".to_string(),
            checkpoint_id: "test".to_string(),
            workbook: "test.md".to_string(),
            next_block: 1,
            line_number: 1,
            section_index: 0,
            kind: None,
            match_: None,
            bind: None,
            created_at: Utc::now().to_rfc3339(),
            // 1 hour in the past
            timeout_at: Some((Utc::now() - ChronoDuration::hours(1)).to_rfc3339()),
            on_timeout: None,
            sidecar_state: None,
            resume_url: None,
            verb_index: None,
            message: None,
            context_url: None,
            resume_on: None,
            timeout: None,
            actions: Vec::new(),
            callback_url: None,
            callback_secret: None,
        };
        assert!(is_expired(&desc));
    }

    #[test]
    fn test_is_expired_future() {
        let desc = PendingDescriptor {
            checkpoint: "test".to_string(),
            checkpoint_id: "test".to_string(),
            workbook: "test.md".to_string(),
            next_block: 1,
            line_number: 1,
            section_index: 0,
            kind: None,
            match_: None,
            bind: None,
            created_at: Utc::now().to_rfc3339(),
            // 1 hour in the future
            timeout_at: Some((Utc::now() + ChronoDuration::hours(1)).to_rfc3339()),
            on_timeout: None,
            sidecar_state: None,
            resume_url: None,
            verb_index: None,
            message: None,
            context_url: None,
            resume_on: None,
            timeout: None,
            actions: Vec::new(),
            callback_url: None,
            callback_secret: None,
        };
        assert!(!is_expired(&desc));
    }

    #[test]
    fn test_is_expired_no_timeout() {
        let desc = PendingDescriptor {
            checkpoint: "test".to_string(),
            checkpoint_id: "test".to_string(),
            workbook: "test.md".to_string(),
            next_block: 1,
            line_number: 1,
            section_index: 0,
            kind: None,
            match_: None,
            bind: None,
            created_at: Utc::now().to_rfc3339(),
            timeout_at: None,
            on_timeout: None,
            sidecar_state: None,
            resume_url: None,
            verb_index: None,
            message: None,
            context_url: None,
            resume_on: None,
            timeout: None,
            actions: Vec::new(),
            callback_url: None,
            callback_secret: None,
        };
        assert!(!is_expired(&desc));
    }

    #[test]
    fn test_summarize_single_bind() {
        let desc = PendingDescriptor {
            checkpoint: "test".to_string(),
            checkpoint_id: "my-run".to_string(),
            workbook: "/path/to/deploy.md".to_string(),
            next_block: 2,
            line_number: 42,
            section_index: 3,
            kind: Some("email".to_string()),
            match_: None,
            bind: Some(BindSpec::Single("otp_code".to_string())),
            created_at: Utc::now().to_rfc3339(),
            timeout_at: Some("2099-01-01T00:00:00+00:00".to_string()),
            on_timeout: Some("abort".to_string()),
            sidecar_state: None,
            resume_url: None,
            verb_index: None,
            message: None,
            context_url: None,
            resume_on: None,
            timeout: None,
            actions: Vec::new(),
            callback_url: None,
            callback_secret: None,
        };
        let s = summarize("my-run", &desc);
        assert!(s.contains("my-run"), "should contain id");
        assert!(s.contains("email"), "should contain kind");
        assert!(s.contains("otp_code"), "should contain bind var");
        assert!(s.contains("deploy.md"), "should contain workbook basename");
        assert!(s.contains("L42"), "should contain line number");
        assert!(!s.contains("[EXPIRED]"), "should not be expired");
    }

    #[test]
    fn test_summarize_multi_bind() {
        let desc = PendingDescriptor {
            checkpoint: "test".to_string(),
            checkpoint_id: "run-2".to_string(),
            workbook: "test.md".to_string(),
            next_block: 1,
            line_number: 10,
            section_index: 1,
            kind: Some("manual".to_string()),
            match_: None,
            bind: Some(BindSpec::Multiple(vec![
                "code".to_string(),
                "sender".to_string(),
            ])),
            created_at: Utc::now().to_rfc3339(),
            timeout_at: None,
            on_timeout: None,
            sidecar_state: None,
            resume_url: None,
            verb_index: None,
            message: None,
            context_url: None,
            resume_on: None,
            timeout: None,
            actions: Vec::new(),
            callback_url: None,
            callback_secret: None,
        };
        let s = summarize("run-2", &desc);
        assert!(s.contains("code,sender"), "should contain joined bind vars");
        assert!(s.contains("never"), "no timeout should show 'never'");
    }

    #[test]
    fn test_summarize_expired() {
        let desc = PendingDescriptor {
            checkpoint: "test".to_string(),
            checkpoint_id: "old-run".to_string(),
            workbook: "test.md".to_string(),
            next_block: 1,
            line_number: 5,
            section_index: 0,
            kind: None,
            match_: None,
            bind: None,
            created_at: Utc::now().to_rfc3339(),
            timeout_at: Some((Utc::now() - ChronoDuration::hours(1)).to_rfc3339()),
            on_timeout: None,
            sidecar_state: None,
            resume_url: None,
            verb_index: None,
            message: None,
            context_url: None,
            resume_on: None,
            timeout: None,
            actions: Vec::new(),
            callback_url: None,
            callback_secret: None,
        };
        let s = summarize("old-run", &desc);
        assert!(s.contains("[EXPIRED]"), "should show expired marker");
    }

    #[test]
    fn test_summarize_no_kind_no_bind() {
        let desc = PendingDescriptor {
            checkpoint: "test".to_string(),
            checkpoint_id: "bare".to_string(),
            workbook: "bare.md".to_string(),
            next_block: 0,
            line_number: 1,
            section_index: 0,
            kind: None,
            match_: None,
            bind: None,
            created_at: Utc::now().to_rfc3339(),
            timeout_at: None,
            on_timeout: None,
            sidecar_state: None,
            resume_url: None,
            verb_index: None,
            message: None,
            context_url: None,
            resume_on: None,
            timeout: None,
            actions: Vec::new(),
            callback_url: None,
            callback_secret: None,
        };
        let s = summarize("bare", &desc);
        // kind defaults to "-", bind defaults to "-"
        assert!(s.contains("bind=-"), "should show dash for missing bind");
    }

    #[test]
    fn test_list_all_returns_sorted() {
        // Use unique prefixed IDs so they sort predictably and don't collide
        let prefix = unique_id("test_pending_list");
        let id_a = format!("{}_aaa", prefix);
        let id_b = format!("{}_bbb", prefix);

        let spec = WaitSpec {
            kind: Some("manual".to_string()),
            attrs: Default::default(),
            ..WaitSpec::default()
        };
        let desc_a = build(&id_a, "a.md", 1, &spec, None);
        let desc_b = build(&id_b, "b.md", 2, &spec, None);

        save(&id_a, &desc_a).expect("save a");
        save(&id_b, &desc_b).expect("save b");

        let all = list_all();
        let our_entries: Vec<_> = all
            .iter()
            .filter(|(id, _)| id.starts_with(&prefix))
            .collect();
        assert_eq!(our_entries.len(), 2, "should find both descriptors");
        assert_eq!(our_entries[0].0, id_a, "first should be _aaa");
        assert_eq!(our_entries[1].0, id_b, "second should be _bbb");
        assert_eq!(our_entries[0].1.workbook, "a.md");
        assert_eq!(our_entries[1].1.workbook, "b.md");

        // Clean up
        delete(&id_a).expect("cleanup a");
        delete(&id_b).expect("cleanup b");
    }

    /// Build an already-expired pending descriptor suitable for reaper tests.
    fn expired_desc(id: &str, workbook: &str, on_timeout: Option<&str>) -> PendingDescriptor {
        PendingDescriptor {
            checkpoint: checkpoint::checkpoint_path(id)
                .to_string_lossy()
                .to_string(),
            checkpoint_id: id.to_string(),
            workbook: workbook.to_string(),
            next_block: 1,
            line_number: 1,
            section_index: 0,
            kind: Some("email".to_string()),
            match_: None,
            bind: Some(BindSpec::Single("otp".to_string())),
            created_at: (Utc::now() - ChronoDuration::hours(2)).to_rfc3339(),
            timeout_at: Some((Utc::now() - ChronoDuration::hours(1)).to_rfc3339()),
            on_timeout: on_timeout.map(String::from),
            sidecar_state: None,
            resume_url: None,
            verb_index: None,
            message: None,
            context_url: None,
            resume_on: None,
            timeout: None,
            actions: Vec::new(),
            callback_url: None,
            callback_secret: None,
        }
    }

    // Note: `reap_expired` scans the shared checkpoint dir, so parallel tests
    // can reap each other's descriptors. Tests assert post-conditions (gone /
    // still present) rather than "my call returned my id", which is racy.

    #[test]
    fn test_reap_expired_abort_mode_is_reaped() {
        let id = unique_id("test_reap_abort");
        // Save checkpoint BEFORE pending: any parallel reaper that sees our
        // pending descriptor will also find the checkpoint. Reversing this
        // order lets a racing reaper delete pending before our ckpt is saved,
        // leaving the ckpt unmarked.
        let ckpt = checkpoint::Checkpoint::new("deploy.md", 3);
        checkpoint::save(&id, &ckpt).expect("save ckpt");
        let desc = expired_desc(&id, "deploy.md", Some("abort"));
        save(&id, &desc).expect("save");

        let _ = reap_expired();

        assert!(
            load(&id).expect("load").is_none(),
            "pending descriptor should be gone"
        );
        let loaded_ckpt = checkpoint::load(&id).expect("load ckpt").expect("ckpt");
        assert_eq!(loaded_ckpt.status, checkpoint::CheckpointStatus::Failed);

        let _ = checkpoint::delete(&id);
    }

    #[test]
    fn test_reap_expired_unset_on_timeout_is_reaped() {
        // Unset on_timeout defaults to abort semantics — should be reaped.
        let id = unique_id("test_reap_unset");
        let desc = expired_desc(&id, "deploy.md", None);
        save(&id, &desc).expect("save");

        let _ = reap_expired();
        assert!(
            load(&id).expect("load").is_none(),
            "unset on_timeout should be reaped"
        );
    }

    #[test]
    fn test_reap_expired_unknown_on_timeout_is_reaped() {
        // Unknown values default to abort on resume, so reaper treats same.
        let id = unique_id("test_reap_unknown");
        let desc = expired_desc(&id, "deploy.md", Some("explode"));
        save(&id, &desc).expect("save");

        let _ = reap_expired();
        assert!(
            load(&id).expect("load").is_none(),
            "unknown on_timeout should be reaped"
        );
    }

    #[test]
    fn test_reap_expired_returns_entry_fields() {
        // Confirm the ReapedEntry shape is populated correctly. This test
        // ran flaky under parallel test pressure because `reap_expired` has
        // no file lock — two concurrent reapers (ours + a parallel test's)
        // can race at the ckpt load/save step, leaving one of them with
        // `checkpoint_marked_failed=false` even though the ckpt ends up
        // Failed on disk. The load-bearing post-conditions are:
        //   1. Our pending descriptor is gone.
        //   2. Our ckpt, if present, is Failed.
        // These hold regardless of which concurrent reaper did the marking.
        let id = unique_id("test_reap_entry_fields");
        let ckpt = checkpoint::Checkpoint::new("fields.md", 2);
        checkpoint::save(&id, &ckpt).expect("save ckpt");
        let desc = expired_desc(&id, "fields.md", Some("abort"));
        save(&id, &desc).expect("save");

        let reaped = reap_expired();

        // Post-condition 1: pending gone.
        assert!(load(&id).expect("load").is_none());
        // Post-condition 2: ckpt is Failed (or absent if a prior cleanup removed it).
        let ckpt_final = checkpoint::load(&id).expect("load ckpt");
        assert!(
            ckpt_final
                .as_ref()
                .is_none_or(|c| c.status == checkpoint::CheckpointStatus::Failed),
            "ckpt should be Failed after reap, got {:?}",
            ckpt_final.as_ref().map(|c| &c.status)
        );
        // If our own call saw the id in its reaped Vec, sanity-check the fields.
        if let Some(ours) = reaped.iter().find(|r| r.id == id) {
            assert_eq!(ours.workbook, "fields.md");
            assert_eq!(ours.on_timeout.as_deref(), Some("abort"));
            assert!(ours.timeout_at.is_some());
        }

        let _ = checkpoint::delete(&id);
    }

    #[test]
    fn test_reap_leaves_skip_mode() {
        // `skip` needs to bind empty vars and keep executing — can't reap
        // without running blocks, so the reaper must skip it.
        let id = unique_id("test_reap_skip");
        let desc = expired_desc(&id, "deploy.md", Some("skip"));
        save(&id, &desc).expect("save");

        let reaped = reap_expired();
        assert!(
            reaped.iter().all(|r| r.id != id),
            "skip mode must not be reaped"
        );
        assert!(
            load(&id).expect("load").is_some(),
            "descriptor should remain"
        );

        delete(&id).expect("cleanup");
    }

    #[test]
    fn test_reap_leaves_prompt_mode() {
        // `prompt` requires an interactive terminal on resume — can't reap.
        let id = unique_id("test_reap_prompt");
        let desc = expired_desc(&id, "deploy.md", Some("prompt"));
        save(&id, &desc).expect("save");

        let reaped = reap_expired();
        assert!(
            reaped.iter().all(|r| r.id != id),
            "prompt mode must not be reaped"
        );
        assert!(load(&id).expect("load").is_some());

        delete(&id).expect("cleanup");
    }

    #[test]
    fn test_reap_leaves_unexpired() {
        let id = unique_id("test_reap_unexpired");
        let mut desc = expired_desc(&id, "deploy.md", Some("abort"));
        // Move timeout into the future.
        desc.timeout_at = Some((Utc::now() + ChronoDuration::hours(1)).to_rfc3339());
        save(&id, &desc).expect("save");

        let reaped = reap_expired();
        assert!(
            reaped.iter().all(|r| r.id != id),
            "unexpired must not be reaped"
        );
        assert!(load(&id).expect("load").is_some());

        delete(&id).expect("cleanup");
    }

    #[test]
    fn test_reap_no_checkpoint_is_ok() {
        // Descriptor without a paired checkpoint should still be deleted,
        // just with checkpoint_marked_failed=false (when our call reaps it).
        let id = unique_id("test_reap_no_ckpt");
        let desc = expired_desc(&id, "deploy.md", Some("abort"));
        save(&id, &desc).expect("save");
        // No checkpoint::save — simulating a descriptor whose checkpoint
        // was already cleaned up (or never existed).

        let reaped = reap_expired();
        if let Some(ours) = reaped.iter().find(|r| r.id == id) {
            assert!(!ours.checkpoint_marked_failed);
        }
        // Post-condition holds regardless of which parallel call reaped it.
        assert!(load(&id).expect("load").is_none());
    }

    #[test]
    fn test_reap_concurrent_reapers_partition_ids() {
        // Two threads call `reap_expired()` simultaneously over the same set
        // of expired descriptors. Without the per-id lock, both threads can
        // race the load → mark_failed → save → delete sequence and both push
        // a `ReapedEntry` for the same id. With the lock, each id is reaped
        // exactly once: its ReapedEntry appears in at most one thread's
        // result, never both.
        //
        // Note: `reap_expired` scans the shared checkpoint dir, so other
        // parallel tests' reapers may also claim our descriptors (the
        // existing single-id reap tests do unconditional `reap_expired()`
        // calls and pick up anything expired). We filter both to our prefix
        // and to whatever survived parallel test pressure, and then assert
        // the load-bearing race property: no id appears in both threads'
        // results.
        use std::collections::HashSet;
        use std::sync::Arc;
        use std::sync::Barrier;

        let prefix = unique_id("test_reap_concurrent");
        let checkpoint_dir = std::env::temp_dir().join(format!("wb_reap_concurrent_{}", prefix));
        let _checkpoint_dir_guard = set_thread_checkpoint_dir(checkpoint_dir.clone());
        const N: usize = 5;
        let mut our_ids: HashSet<String> = HashSet::new();
        for n in 0..N {
            let id = format!("{}_{}", prefix, n);
            // Pair each pending with a checkpoint so the load/mark_failed/save
            // path inside the lock actually does work — that's the side that
            // races without locking.
            let ckpt = checkpoint::Checkpoint::new("race.md", 1);
            checkpoint::save(&id, &ckpt).expect("save ckpt");
            let desc = expired_desc(&id, "race.md", Some("abort"));
            save(&id, &desc).expect("save pending");
            our_ids.insert(id);
        }

        // Barrier ensures both threads enter `reap_expired` close enough in
        // time to genuinely race on the same descriptors.
        let barrier = Arc::new(Barrier::new(2));
        let b1 = Arc::clone(&barrier);
        let b2 = Arc::clone(&barrier);
        let t1_checkpoint_dir = checkpoint_dir.clone();
        let t2_checkpoint_dir = checkpoint_dir.clone();

        let t1 = std::thread::spawn(move || {
            let _checkpoint_dir_guard = set_thread_checkpoint_dir(t1_checkpoint_dir);
            b1.wait();
            reap_expired()
        });
        let t2 = std::thread::spawn(move || {
            let _checkpoint_dir_guard = set_thread_checkpoint_dir(t2_checkpoint_dir);
            b2.wait();
            reap_expired()
        });

        let r1 = t1.join().expect("t1 join");
        let r2 = t2.join().expect("t2 join");

        let r1_ours: HashSet<String> = r1
            .iter()
            .filter(|e| our_ids.contains(&e.id))
            .map(|e| e.id.clone())
            .collect();
        let r2_ours: HashSet<String> = r2
            .iter()
            .filter(|e| our_ids.contains(&e.id))
            .map(|e| e.id.clone())
            .collect();

        // Load-bearing assertion: each id must appear in *at most one*
        // thread's result. The lock serializes reapers so the loser sees the
        // descriptor already gone and skips it (no spurious ReapedEntry).
        // Without the lock, both threads happily push the same id.
        let intersection: HashSet<&String> = r1_ours.intersection(&r2_ours).collect();
        assert!(
            intersection.is_empty(),
            "id reaped by both threads (lock failed): {:?}",
            intersection
        );

        // Sanity: at least *some* of our ids were reaped between the two
        // threads (we created 5; even if other parallel tests stole a few,
        // at least one of ours should land in our threads' results, since
        // those threads run concurrently with whatever else is going on).
        // This guards against a future change that accidentally short-
        // circuits the reaper for our prefix.
        let our_reaped_count = r1_ours.len() + r2_ours.len();
        assert!(
            our_reaped_count >= 1,
            "neither thread reaped any of our {} ids — reaper short-circuited?",
            N
        );

        // Post-condition: every descriptor we created is gone from disk,
        // regardless of which reaper (ours or a sibling test's) cleaned it up.
        for id in &our_ids {
            assert!(
                load(id).expect("load").is_none(),
                "descriptor {} survived concurrent reap",
                id
            );
        }

        // Cleanup checkpoints (descriptors are already gone).
        for id in &our_ids {
            let _ = checkpoint::delete(id);
        }
    }

    fn make_browser_slice_spec() -> parser::BrowserSliceSpec {
        parser::BrowserSliceSpec {
            line_number: 17,
            section_index: 2,
            ..Default::default()
        }
    }

    #[test]
    fn test_build_for_browser_pause_threads_pause_for_human_fields() {
        let slice = make_browser_slice_spec();
        let pause = crate::sidecar::PauseInfo {
            sidecar_state: None,
            reason: Some("pause_for_human".into()),
            resume_url: Some("https://run.example.com/runs/xyz".into()),
            verb_index: Some(3),
            message: Some("Drop receipts".into()),
            context_url: Some("https://drive.google.com/x".into()),
            resume_on: Some("operator_click".into()),
            timeout: Some("1h".into()),
            actions: vec![serde_json::json!({"label": "OK", "value": "ok"})],
        };
        let desc = build_for_browser_pause("ckpt-1", "t.md", 5, &slice, &pause, None);
        assert_eq!(desc.checkpoint_id, "ckpt-1");
        assert_eq!(desc.next_block, 5);
        assert_eq!(desc.line_number, 17);
        assert_eq!(desc.verb_index, Some(3));
        assert_eq!(desc.message.as_deref(), Some("Drop receipts"));
        assert_eq!(
            desc.context_url.as_deref(),
            Some("https://drive.google.com/x")
        );
        assert_eq!(desc.resume_on.as_deref(), Some("operator_click"));
        assert_eq!(desc.timeout.as_deref(), Some("1h"));
        assert_eq!(desc.actions.len(), 1);
        // timeout: "1h" with resume_on: operator_click → timeout_at is set
        // (for display/reaper scheduling) but on_timeout stays unset (reaper
        // doesn't auto-abort an operator_click pause).
        assert!(desc.timeout_at.is_some());
        assert!(desc.on_timeout.is_none());
    }

    #[test]
    fn test_build_for_browser_pause_timeout_mode_sets_on_timeout_abort() {
        // resume_on: timeout means "no operator needed; auto-abort on expiry."
        // The reaper sweep in `wb pending` relies on on_timeout == "abort" to
        // fire the auto-cleanup; we set it here so the contract is automatic
        // for workbook authors.
        let slice = make_browser_slice_spec();
        let pause = crate::sidecar::PauseInfo {
            resume_on: Some("timeout".into()),
            timeout: Some("30s".into()),
            ..Default::default()
        };
        let desc = build_for_browser_pause("ckpt-1", "t.md", 0, &slice, &pause, None);
        assert_eq!(desc.on_timeout.as_deref(), Some("abort"));
        assert!(desc.timeout_at.is_some());
    }

    #[test]
    fn test_build_for_browser_pause_round_trips_through_json() {
        // Critical invariant: the descriptor must survive a save+load cycle
        // so timeout reaping and resume can read the new fields off disk.
        let slice = make_browser_slice_spec();
        let pause = crate::sidecar::PauseInfo {
            message: Some("m".into()),
            context_url: Some("u".into()),
            resume_on: Some("poll".into()),
            actions: vec![serde_json::json!({"label": "Go", "value": 1})],
            ..Default::default()
        };
        let desc = build_for_browser_pause("ckpt-rt", "t.md", 0, &slice, &pause, None);
        let serialized = serde_json::to_string(&desc).expect("serialize");
        let back: PendingDescriptor = serde_json::from_str(&serialized).expect("deserialize");
        assert_eq!(back.message.as_deref(), Some("m"));
        assert_eq!(back.context_url.as_deref(), Some("u"));
        assert_eq!(back.resume_on.as_deref(), Some("poll"));
        assert_eq!(back.actions.len(), 1);
        assert_eq!(back.actions[0]["value"], 1);
    }

    #[test]
    fn test_legacy_descriptor_without_new_fields_parses() {
        // Descriptors written by older `wb` versions won't have the
        // pause_for_human fields. `#[serde(default)]` on each must be wired
        // so loading them doesn't error — otherwise an upgrade strands
        // every in-flight paused workbook.
        let legacy = r#"{
            "checkpoint": "c",
            "checkpoint_id": "ckpt-old",
            "workbook": "w.md",
            "next_block": 1,
            "line_number": 1,
            "section_index": 0,
            "created_at": "2026-01-01T00:00:00Z"
        }"#;
        let desc: PendingDescriptor = serde_json::from_str(legacy).expect("parse legacy");
        assert_eq!(desc.checkpoint_id, "ckpt-old");
        assert!(desc.message.is_none());
        assert!(desc.actions.is_empty());
        // The new callback persistence fields default to None for legacy
        // descriptors written before this feature shipped.
        assert!(desc.callback_url.is_none());
        assert!(desc.callback_secret.is_none());
    }

    #[test]
    fn test_build_persists_callback_url_and_secret() {
        let spec = make_wait_spec();
        let desc = build(
            "ckpt-cb",
            "deploy.md",
            2,
            &spec,
            Some(("https://hooks.example.com/wb", Some("topsecret"))),
        );
        assert_eq!(
            desc.callback_url.as_deref(),
            Some("https://hooks.example.com/wb")
        );
        assert_eq!(desc.callback_secret.as_deref(), Some("topsecret"));
    }

    #[test]
    fn test_build_no_callback_leaves_fields_none() {
        let spec = make_wait_spec();
        let desc = build("ckpt-nocb", "deploy.md", 2, &spec, None);
        assert!(desc.callback_url.is_none());
        assert!(desc.callback_secret.is_none());
    }

    #[test]
    fn test_callback_fields_round_trip_through_json() {
        let spec = make_wait_spec();
        let desc = build(
            "ckpt-rt-cb",
            "w.md",
            1,
            &spec,
            Some(("https://hooks.example.com/wb", Some("s"))),
        );
        let serialized = serde_json::to_string(&desc).expect("serialize");
        // skip_serializing_if keeps the JSON tight — but the values must
        // survive a load round-trip.
        let back: PendingDescriptor = serde_json::from_str(&serialized).expect("deserialize");
        assert_eq!(
            back.callback_url.as_deref(),
            Some("https://hooks.example.com/wb")
        );
        assert_eq!(back.callback_secret.as_deref(), Some("s"));
    }

    #[test]
    fn test_reap_expired_fires_checkpoint_failed_callback() {
        // Bind an ephemeral port and accept one connection. The reaper
        // should POST `checkpoint.failed` to the listener when it sweeps
        // an expired descriptor whose callback_url is set.
        use std::io::Read;
        use std::net::TcpListener;
        use std::sync::mpsc;
        use std::thread;
        use std::time::Duration as StdDuration;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().expect("local_addr").port();
        let url = format!("http://127.0.0.1:{}/hook", port);

        let (tx, rx) = mpsc::channel::<String>();
        let handle = thread::spawn(move || {
            listener.set_nonblocking(false).expect("set blocking");
            // Accept one connection — the reaper's curl call.
            if let Ok((mut stream, _)) = listener.accept() {
                let _ = stream.set_read_timeout(Some(StdDuration::from_secs(2)));
                let mut buf = [0u8; 8192];
                let mut accumulated = String::new();
                // Read what we can; curl will close the write half once it's
                // done sending. We don't need the full body — headers are
                // enough to verify the event.
                loop {
                    match stream.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            accumulated.push_str(&String::from_utf8_lossy(&buf[..n]));
                            // Once we've seen the headers' double-CRLF we
                            // can stop — payload is large but the headers
                            // are what we check.
                            if accumulated.contains("\r\n\r\n") {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                // Minimal HTTP/1.1 response so curl returns a 2xx.
                use std::io::Write;
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                );
                let _ = tx.send(accumulated);
            }
        });

        // Build an expired descriptor that points at the listener.
        let id = unique_id("test_reap_callback");
        let mut desc = expired_desc(&id, "deploy.md", Some("abort"));
        desc.callback_url = Some(url.clone());
        desc.callback_secret = Some("hmackey".to_string());
        save(&id, &desc).expect("save");

        // Save a checkpoint so reap_expired marks it failed (and exercises
        // the total_blocks branch of the reaper).
        let ckpt = checkpoint::Checkpoint::new("deploy.md", 5);
        checkpoint::save(&id, &ckpt).expect("save ckpt");

        let _ = reap_expired();

        // Wait briefly for the listener thread to relay what it saw.
        let received = rx
            .recv_timeout(StdDuration::from_secs(5))
            .expect("listener should receive a request");
        let _ = handle.join();

        assert!(
            received.contains("X-WB-Event: checkpoint.failed"),
            "expected X-WB-Event: checkpoint.failed header, got:\n{}",
            received
        );
        // HMAC signature header is present when secret is set.
        assert!(
            received.contains("X-WB-Signature: sha256="),
            "expected signed payload, got:\n{}",
            received
        );

        // Descriptor still gone after callback fired.
        assert!(load(&id).expect("load").is_none());
        let _ = checkpoint::delete(&id);
    }

    #[test]
    fn test_reap_expired_without_callback_url_does_not_panic() {
        // Sanity: descriptor without a callback_url means no HTTP call.
        // The reap should still mark the checkpoint failed and delete the
        // descriptor.
        let id = unique_id("test_reap_no_cb_url");
        let ckpt = checkpoint::Checkpoint::new("deploy.md", 1);
        checkpoint::save(&id, &ckpt).expect("save ckpt");
        let desc = expired_desc(&id, "deploy.md", Some("abort"));
        save(&id, &desc).expect("save");

        let _ = reap_expired();
        assert!(load(&id).expect("load").is_none());
        let _ = checkpoint::delete(&id);
    }
}
