# Upgrading - TODO

## Setup & Configuration

- [ ] Add `tauri-plugin-updater` dependency to `src-tauri/Cargo.toml`
- [ ] Generate signing key pair for release artifacts
- [ ] Configure updater in `tauri.conf.json` with public key
- [ ] Add `semver` crate dependency for version comparison
- [ ] Add `reqwest` for GitHub API calls (if not already present)

## Backend (Rust)

- [ ] Create `src-tauri/src/updater.rs` module
- [ ] Implement `UpdateManager` struct
- [ ] Implement GitHub Releases API client
- [ ] Implement version comparison using semver
- [ ] Implement update cache (read/write `~/.workbooks/update_cache.json`)
- [ ] Implement background update checker (every 24 hours)
- [ ] Add UpdateManager to AppState
- [ ] Register updater Tauri commands in lib.rs
- [ ] Integrate with NotificationManager (create update notifications)
- [ ] Add update check on app startup
- [ ] Emit events to frontend when update available

## Frontend (React)

- [ ] Create `UpdateBanner.jsx` component (top banner notification)
- [ ] Create `ChangelogModal.jsx` component (view release notes)
- [ ] Create `UpdateProgress.jsx` component (download/install progress)
- [ ] Add update check listener (event from backend)
- [ ] Implement "Update Now" click handler
- [ ] Implement "View Changes" click handler
- [ ] Show update progress during download/install
- [ ] Handle update errors gracefully
- [ ] Add update preferences to Settings page

## Tray Integration

- [ ] Add "Update Available" tray menu item when update detected
- [ ] Update tray menu dynamically when update status changes
- [ ] Add click handler to open app and show update dialog

## CI/CD & Release Automation

- [ ] Create `.github/workflows/release.yml` for automated releases
- [ ] Configure build matrix for all platforms (macOS, Windows, Linux)
- [ ] Add artifact signing step in CI
- [ ] Generate `latest.json` manifest in CI
- [ ] Upload artifacts and manifest to GitHub releases
- [ ] Update `npm run version` script to create git tags
- [ ] Test full release process on staging

## Settings/Preferences

- [ ] Add updater preferences section in Settings
- [ ] Enable/disable automatic update checks toggle
- [ ] Auto-install vs. notify only toggle
- [ ] Check for beta/pre-release versions option
- [ ] Update check frequency setting (daily/weekly/manual)

## Testing

- [ ] Test update check on app startup
- [ ] Test background update check (24-hour cycle)
- [ ] Test manual "Check for Updates" from menu
- [ ] Test update notification creation
- [ ] Test update download and install flow
- [ ] Test signature verification
- [ ] Test update cache functionality
- [ ] Test error handling (network errors, invalid signatures, etc.)
- [ ] Test update across different platforms (macOS, Windows, Linux)

## Documentation

- [ ] Document release process in CONTRIBUTING.md
- [ ] Document version bumping workflow
- [ ] Document CI/CD pipeline and signing setup
- [ ] Add upgrade system to user guide
