use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
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
}

pub fn descriptor_path(id: &str) -> PathBuf {
    checkpoint::checkpoint_dir().join(format!("{}.pending.json", id))
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

        let mut marked = false;
        if let Ok(Some(mut ckpt)) = checkpoint::load(&id) {
            ckpt.mark_failed();
            if checkpoint::save(&id, &ckpt).is_ok() {
                marked = true;
            }
        }
        let _ = delete(&id);

        reaped.push(ReapedEntry {
            id: id.clone(),
            workbook: desc.workbook.clone(),
            kind: desc.kind.clone(),
            on_timeout: desc.on_timeout.clone(),
            timeout_at: desc.timeout_at.clone(),
            checkpoint_marked_failed: marked,
        });
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
        }
    }

    #[test]
    fn test_build_creates_correct_descriptor() {
        let spec = make_wait_spec();
        let desc = build("ckpt-1", "deploy.md", 2, &spec);

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
        };
        let desc = build("ckpt-2", "manual.md", 1, &spec);
        assert!(desc.timeout_at.is_none());
        assert!(desc.on_timeout.is_none());
        assert!(desc.bind.is_none());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let id = unique_id("test_pending_roundtrip");
        let spec = make_wait_spec();
        let desc = build(&id, "deploy.md", 2, &spec);

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
        let desc = build(&id, "deploy.md", 2, &spec);

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
            ..WaitSpec::default()
        };
        let desc_a = build(&id_a, "a.md", 1, &spec);
        let desc_b = build(&id_b, "b.md", 2, &spec);

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
        let desc = build_for_browser_pause("ckpt-1", "t.md", 5, &slice, &pause);
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
        let desc = build_for_browser_pause("ckpt-1", "t.md", 0, &slice, &pause);
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
        let desc = build_for_browser_pause("ckpt-rt", "t.md", 0, &slice, &pause);
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
    }
}
