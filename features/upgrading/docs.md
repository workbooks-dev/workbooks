# Upgrading

Automatic update checking and seamless in-app upgrades for Workbooks.

## Overview

Workbooks checks for new versions every 24 hours, notifies users when updates are available, and provides one-click auto-updates using Tauri's built-in updater.

## Update Sources

**GitHub Releases**
- Primary source: https://github.com/YOUR_ORG/workbooks/releases
- Uses GitHub Releases API to check for new versions
- Downloads release artifacts (`.app`, `.dmg`, `.AppImage`, `.msi`, etc.)
- Validates signatures for security

## Update Check Strategy

### When to Check
1. **On app startup** - Quick background check
2. **Every 24 hours** - While app is running
3. **Manual check** - User clicks "Check for Updates" in menu/settings

### Caching
- Cache last check timestamp in `~/.workbooks/update_cache.json`
- Don't re-check if checked within last 24 hours
- Manual check always bypasses cache

### Process
1. Fetch latest release from GitHub API: `GET /repos/{owner}/{repo}/releases/latest`
2. Parse version from release tag (e.g., `v0.2.0` → `0.2.0`)
3. Compare with current version using semver rules
4. If newer version available:
   - Create notification via NotificationManager
   - Update tray menu to show "Update Available"
   - Store update info in cache

## Version Comparison

Use **semver** crate for version comparison:
- `0.1.0` < `0.2.0` (minor update)
- `0.2.0` < `1.0.0` (major update)
- `0.1.0` < `0.1.1` (patch update)

Tag format: `v{MAJOR}.{MINOR}.{PATCH}` (e.g., `v0.1.0`)

## Auto-Update Implementation

### Tauri Updater Plugin

Use **`tauri-plugin-updater`** for secure, automatic updates:
- Downloads new version in background
- Verifies signature
- Installs and restarts app
- Platform-specific installers:
  - **macOS**: `.app` bundle or `.dmg`
  - **Windows**: `.msi` or `.exe` installer
  - **Linux**: `.AppImage` or `.deb`

### Configuration

**`tauri.conf.json`**:
```json
{
  "plugins": {
    "updater": {
      "active": true,
      "endpoints": [
        "https://github.com/YOUR_ORG/workbooks/releases/latest/download/latest.json"
      ],
      "dialog": false,
      "pubkey": "YOUR_PUBLIC_KEY_HERE"
    }
  }
}
```

**Update manifest** (`latest.json` in GitHub releases):
```json
{
  "version": "0.2.0",
  "notes": "New features: X, Y, Z. Bug fixes: A, B, C.",
  "pub_date": "2025-01-15T12:00:00Z",
  "platforms": {
    "darwin-x86_64": {
      "signature": "...",
      "url": "https://github.com/YOUR_ORG/workbooks/releases/download/v0.2.0/workbooks_0.2.0_x64.app.tar.gz"
    },
    "darwin-aarch64": {
      "signature": "...",
      "url": "https://github.com/YOUR_ORG/workbooks/releases/download/v0.2.0/workbooks_0.2.0_aarch64.app.tar.gz"
    },
    "linux-x86_64": {
      "signature": "...",
      "url": "https://github.com/YOUR_ORG/workbooks/releases/download/v0.2.0/workbooks_0.2.0_amd64.AppImage"
    },
    "windows-x86_64": {
      "signature": "...",
      "url": "https://github.com/YOUR_ORG/workbooks/releases/download/v0.2.0/workbooks_0.2.0_x64_en-US.msi"
    }
  }
}
```

### Signature Verification

Generate key pair for signing releases:
```bash
# One-time setup
tauri signer generate -w ~/.tauri/workbooks.key

# Sign release artifacts during CI/CD
tauri signer sign /path/to/workbooks.app --private-key ~/.tauri/workbooks.key
```

Store public key in `tauri.conf.json` → updater plugin validates signatures before installing.

## Update Manager (Rust)

**Location**: `src-tauri/src/updater.rs`

```rust
use semver::Version;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize, Serialize)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub release_notes: String,
    pub download_url: String,
    pub published_at: String,
}

#[derive(Deserialize, Serialize)]
struct UpdateCache {
    last_check: i64,          // Unix timestamp
    update_available: bool,
    latest_version: Option<String>,
}

pub struct UpdateManager {
    cache_path: PathBuf,
    github_owner: String,
    github_repo: String,
}

impl UpdateManager {
    pub fn new() -> Result<Self>;

    // Check for updates
    pub async fn check_for_updates(&self, force: bool) -> Result<Option<UpdateInfo>>;

    // Get cached update info (if checked recently)
    pub fn get_cached_update(&self) -> Result<Option<UpdateInfo>>;

    // Download and install update using Tauri updater
    pub async fn install_update(&self, app: &tauri::AppHandle) -> Result<()>;

    // Parse GitHub release to UpdateInfo
    async fn fetch_latest_release(&self) -> Result<UpdateInfo>;

    // Compare versions
    fn is_newer_version(&self, current: &str, latest: &str) -> bool;

    // Update cache
    fn update_cache(&self, cache: &UpdateCache) -> Result<()>;
}
```

## Tauri Commands

```rust
#[tauri::command]
async fn check_for_updates(force: bool, state: State<AppState>) -> Result<Option<UpdateInfo>, String>;

#[tauri::command]
async fn install_update(app: tauri::AppHandle, state: State<AppState>) -> Result<(), String>;

#[tauri::command]
async fn get_current_version() -> Result<String, String> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}
```

## UI Flow

### Update Available Notification

When update is detected:
1. Create notification: "Update available: v0.1.0 → v0.2.0"
2. Update tray menu: Add "⬆️ Update Available" item at top
3. Show in-app banner (if app is open)

### Update Dialog/Banner

**UpdateBanner.jsx** - Top banner in app:
- "Workbooks v0.2.0 is available. You're on v0.1.0."
- **[View Changes]** → Opens changelog modal
- **[Update Now]** → Triggers download and install
- **[Dismiss]** → Hides banner until next version

**ChangelogModal.jsx**:
- Shows release notes in formatted markdown
- Highlights breaking changes, new features, bug fixes
- **[Install Update]** and **[Cancel]** buttons

### Update Progress

While downloading/installing:
1. Show progress modal with:
   - Download progress bar
   - Status: "Downloading update..." / "Installing..." / "Restarting..."
2. Use Tauri updater events to update progress
3. On completion: Auto-restart app

### Tray Menu Items

When update available:
```
⬆️ Update to v0.2.0
────────────────
📋 Recent Notifications
...
```

Click "Update to v0.2.0" → Opens app with update dialog

## Background Update Check

**Scheduled check every 24 hours**:
```rust
// In app setup
tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(24 * 60 * 60)).await;

        if let Ok(update_manager) = UpdateManager::new() {
            if let Ok(Some(update)) = update_manager.check_for_updates(false).await {
                // Create notification
                notification_manager.notify_update_available(
                    &update.current_version,
                    &update.latest_version,
                    &update.release_notes
                );

                // Emit event to frontend
                app.emit("update-available", update);
            }
        }
    }
});
```

## Settings/Preferences

**Update settings** in Settings UI:
- [ ] Enable automatic update checks
- [ ] Auto-install updates (vs. notify only)
- [ ] Check for pre-release/beta versions
- Check frequency: Daily / Weekly / Manual only

## Release Process

### CI/CD Pipeline (GitHub Actions)

**`.github/workflows/release.yml`**:
1. Triggered on tag push: `v*.*.*`
2. Build artifacts for all platforms (macOS, Windows, Linux)
3. Sign artifacts with private key
4. Generate `latest.json` manifest
5. Create GitHub release with:
   - Artifacts (`.app.tar.gz`, `.msi`, `.AppImage`, etc.)
   - Signatures (`.sig` files)
   - `latest.json` manifest
   - Release notes from CHANGELOG.md

### Version Bump Script

Update `npm run version` to also:
- Update `Cargo.toml` version
- Update `tauri.conf.json` version
- Commit with message: "Bump version to vX.Y.Z"
- Create git tag: `vX.Y.Z`

```bash
npm run version 0.2.0
git push && git push --tags
# CI/CD automatically builds and releases
```

## Security Considerations

1. **Signature verification** - Tauri updater verifies all downloads
2. **HTTPS only** - All downloads over encrypted connection
3. **Public key pinning** - Public key in app config, can't be changed by attacker
4. **Hash verification** - Optional additional integrity check

## Error Handling

### Update Check Failures
- Network error → Silently fail, try again next cycle
- GitHub API rate limit → Back off, try later
- Invalid response → Log error, notify user if manual check

### Update Install Failures
- Download error → Retry up to 3 times
- Signature mismatch → Abort, show error
- Insufficient permissions → Guide user to manual install
- Insufficient disk space → Show error, clear cache

## Future Enhancements

- **Delta updates** - Only download changed files (smaller downloads)
- **Rollback** - Revert to previous version if update breaks
- **Beta channel** - Opt-in to pre-release versions
- **Release notes in-app** - Formatted changelog viewer
- **Update scheduling** - Install on next restart, not immediately
- **Partial updates** - Update CLI separately from GUI
