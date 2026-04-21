//! Atomic, mode-restricted file writes for checkpoint/pending state.
//!
//! `write_secret_file` writes to a sibling `.tmp` file and `rename()`s into
//! place, so a SIGKILL mid-write leaves the destination either untouched or
//! fully replaced — never half-written. On Unix, the final file is chmodded
//! to 0o600 because these files routinely contain captured stdout, OTPs, and
//! resolved secret env values, and the default umask would make them
//! world-readable on multi-user hosts.
//!
//! `FileLock` provides an exclusive, try-lock-or-fail advisory lock on a
//! sibling `.lock` file. The lock releases automatically when the returned
//! guard drops — including when the process exits uncleanly — so stale locks
//! cannot wedge future runs.

use fs4::fs_std::FileExt;
use std::path::Path;

pub fn write_secret_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = tmp_sibling(path);
    std::fs::write(&tmp, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

pub struct FileLock {
    _file: std::fs::File,
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self._file);
    }
}

/// Try to acquire an exclusive advisory lock scoped to `path`. Returns an
/// error (not a block) if another process holds the lock — concurrent
/// `wb run --checkpoint <same-id>` callers see a loud failure instead of
/// silently clobbering each other's state.
pub fn try_lock_for(path: &Path) -> std::io::Result<FileLock> {
    let lock_path = lock_sibling(path);
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;
    FileExt::try_lock_exclusive(&file).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            format!("checkpoint already in use by another process: {}", e),
        )
    })?;
    Ok(FileLock { _file: file })
}

fn lock_sibling(path: &Path) -> std::path::PathBuf {
    let stem = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "wb".to_string());
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!(".{}.lock", stem))
}

fn tmp_sibling(path: &Path) -> std::path::PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let stem = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "wb".to_string());
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!(".{}.{}.{}.tmp", stem, pid, nanos))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tempdir() -> std::path::PathBuf {
        let base = std::env::temp_dir();
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = base.join(format!("wb_atomic_io_{}_{}", std::process::id(), n));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writes_contents_atomically() {
        let dir = tempdir();
        let path = dir.join("state.json");
        write_secret_file(&path, b"hello").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn overwrites_existing() {
        let dir = tempdir();
        let path = dir.join("state.json");
        std::fs::write(&path, b"old").unwrap();
        write_secret_file(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn leaves_no_tmp_on_success() {
        let dir = tempdir();
        let path = dir.join("state.json");
        write_secret_file(&path, b"payload").unwrap();
        let leftover: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(leftover.is_empty(), "tmp files left behind: {:?}", leftover);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn sets_0600_perms_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir();
        let path = dir.join("state.json");
        write_secret_file(&path, b"secret").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 0600, got {:o}", mode);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lock_can_be_acquired_and_released() {
        // Intra-process flock is permissive on BSD (macOS); the lock's real
        // job is to coordinate across `wb` processes. This test just
        // exercises the happy path — cross-process contention is covered by
        // the subprocess-spawning integration test `lock_blocks_subprocess`.
        let dir = tempdir();
        let path = dir.join("state.json");
        let guard = try_lock_for(&path).expect("lock should succeed");
        drop(guard);
        let again = try_lock_for(&path).expect("lock should be reacquirable after drop");
        drop(again);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lock_blocks_subprocess() {
        // Cross-process is the real scenario: a second `wb` invocation must
        // see the lock and fail fast. We hold the lock in this test process
        // and spawn a child that tries to acquire it — the child's attempt
        // should fail.
        let dir = tempdir();
        let path = dir.join("state.json");
        let _guard = try_lock_for(&path).expect("first lock should succeed");

        let lock_path = dir.join(".state.json.lock");
        // Python is simpler than spawning a Rust helper binary from a unit test.
        let py = format!(
            "import fcntl, sys\nf = open(r'{}', 'r+')\ntry:\n  fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)\n  sys.exit(0)\nexcept BlockingIOError:\n  sys.exit(1)\n",
            lock_path.display()
        );
        let status = std::process::Command::new("python3")
            .arg("-c")
            .arg(&py)
            .status();
        match status {
            Ok(s) => assert_eq!(
                s.code(),
                Some(1),
                "subprocess should fail to acquire lock while held"
            ),
            Err(_) => {
                // python3 unavailable — skip rather than fail in hermetic envs.
                eprintln!("skipping cross-process lock test: python3 not found");
            }
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn partial_tmp_does_not_affect_target() {
        // Simulate a stale tmp file from a prior crashed run. A subsequent
        // write_secret_file run (with its own tmp name) must not read, rely on,
        // or be confused by it.
        let dir = tempdir();
        let path = dir.join("state.json");
        std::fs::write(&path, b"good").unwrap();
        // Leftover crash-tmp from a hypothetical prior process.
        let mut stale = std::fs::File::create(dir.join(".state.json.99999.0.tmp")).unwrap();
        stale.write_all(b"CORRUPT").unwrap();
        drop(stale);
        write_secret_file(&path, b"fresh").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"fresh");
        std::fs::remove_dir_all(&dir).ok();
    }
}
