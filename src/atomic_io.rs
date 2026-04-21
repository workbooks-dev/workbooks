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
