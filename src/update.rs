use std::env;
use std::fs;
use std::process::Command;

const REPO: &str = "workbooks-dev/workbooks";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn cmd_update(check_only: bool) {
    let current = CURRENT_VERSION;
    eprintln!("wb v{}", current);

    let latest = match fetch_latest_version() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: could not check for updates: {}", e);
            std::process::exit(1);
        }
    };

    let latest_clean = latest.trim_start_matches('v');

    if latest_clean == current {
        eprintln!("up to date");
        return;
    }

    eprintln!("update available: v{} -> v{}", current, latest_clean);

    if check_only {
        return;
    }

    // Determine platform
    let os = detect_os();
    let arch = detect_arch();
    let asset = format!("wb-{}-{}", os, arch);
    let url = format!(
        "https://github.com/{}/releases/download/{}/{}",
        REPO, latest, asset
    );

    eprintln!("downloading {}...", asset);

    // Download to temp file
    let tmp = env::temp_dir().join("wb-update");
    let status = download(&url, &tmp);
    if !status {
        // Fallback: try the install script
        eprintln!("binary download failed, trying install script...");
        let install_status = Command::new("sh")
            .args(["-c", "curl -fsSL https://get.workbooks.dev | sh"])
            .status();
        match install_status {
            Ok(s) if s.success() => {
                eprintln!("updated via install script");
                return;
            }
            _ => {
                eprintln!("error: update failed");
                std::process::exit(1);
            }
        }
    }

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755));
    }

    // Replace current binary
    let current_exe = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot determine binary path: {}", e);
            eprintln!("downloaded to: {}", tmp.display());
            std::process::exit(1);
        }
    };

    // Try direct replacement
    match replace_binary(&tmp, &current_exe) {
        Ok(_) => {
            let _ = fs::remove_file(&tmp);
            eprintln!("updated to v{}", latest_clean);
        }
        Err(e) => {
            // Try with sudo
            eprintln!("need elevated permissions: {}", e);
            let status = Command::new("sudo")
                .args(["mv", &tmp.to_string_lossy(), &current_exe.to_string_lossy()])
                .status();
            match status {
                Ok(s) if s.success() => {
                    eprintln!("updated to v{}", latest_clean);
                }
                _ => {
                    eprintln!("error: could not replace binary");
                    eprintln!("downloaded to: {}", tmp.display());
                    std::process::exit(1);
                }
            }
        }
    }
}

pub fn cmd_version() {
    println!("wb v{}", CURRENT_VERSION);
}

fn fetch_latest_version() -> Result<String, String> {
    // Use curl to hit GitHub API (avoids adding reqwest as a dependency)
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "-H", "Accept: application/vnd.github.v3+json",
            &format!("https://api.github.com/repos/{}/releases/latest", REPO),
        ])
        .output()
        .map_err(|e| format!("curl failed: {}", e))?;

    if !output.status.success() {
        return Err("GitHub API request failed".to_string());
    }

    let body = String::from_utf8_lossy(&output.stdout);

    // Simple JSON extraction — avoid pulling in a JSON parser just for this
    // Look for "tag_name": "v0.1.0"
    for line in body.lines() {
        let line = line.trim();
        if line.starts_with("\"tag_name\"") {
            if let Some(start) = line.find(": \"") {
                let rest = &line[start + 3..];
                if let Some(end) = rest.find('"') {
                    return Ok(rest[..end].to_string());
                }
            }
        }
    }

    Err("could not parse version from GitHub response".to_string())
}

fn download(url: &str, dest: &std::path::Path) -> bool {
    // Try curl first
    let status = Command::new("curl")
        .args(["-fsSL", url, "-o", &dest.to_string_lossy()])
        .status();

    if let Ok(s) = status {
        if s.success() {
            return true;
        }
    }

    // Try wget
    let status = Command::new("wget")
        .args(["-q", url, "-O", &dest.to_string_lossy()])
        .status();

    matches!(status, Ok(s) if s.success())
}

fn replace_binary(
    src: &std::path::Path,
    dest: &std::path::Path,
) -> Result<(), String> {
    // Rename old binary first (atomic on same filesystem)
    let backup = dest.with_extension("old");
    let _ = fs::remove_file(&backup); // ignore if doesn't exist
    fs::rename(dest, &backup).map_err(|e| format!("rename current: {}", e))?;
    match fs::rename(src, dest) {
        Ok(_) => {
            let _ = fs::remove_file(&backup);
            Ok(())
        }
        Err(e) => {
            // Restore backup
            let _ = fs::rename(&backup, dest);
            Err(format!("replace: {}", e))
        }
    }
}

fn detect_os() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "unknown"
    }
}

fn detect_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    }
}
