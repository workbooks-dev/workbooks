# Upgrading - Open Questions

## GitHub Repository

- **Q: What is the GitHub org/repo name for Tether?**
  - Needed for: GitHub Releases API endpoint configuration
  - Format: `github.com/{owner}/{repo}` (e.g., `github.com/paracord/tether`)
  - Used in: `src-tauri/src/updater.rs`, CI/CD workflows

## Release Signing

- **Q: Where should we store the signing private key?**
  - Options:
    1. GitHub Secrets (for CI/CD)
    2. Local secure storage (for local releases)
  - Security: Private key must never be committed to repo

## Auto-Install Behavior

- **Q: Should updates auto-install by default, or just notify?**
  - Option A: Notify only → User clicks "Install" → Download & install
  - Option B: Auto-download → Prompt user to install → Restart
  - Option C: Fully automatic (download, install, restart with user consent)
  - Recommendation: Start with Option A (safest), add B/C as settings later

## Update Channels

- **Q: Do we want beta/pre-release update channels from the start?**
  - If yes: Need separate GitHub release tags (e.g., `v0.2.0-beta.1`)
  - If no: Can add later as a feature
  - Recommendation: Skip for MVP, add later

## CLI Auto-Update

- **Q: Should the CLI auto-update separately from the GUI app?**
  - Current: CLI is bundled with app, re-installed when app updates
  - Alternative: CLI has its own update mechanism
  - Recommendation: Keep bundled for simplicity (CLI version matches app version)

## Update Frequency Override

- **Q: Should power users be able to check more/less frequently than 24 hours?**
  - Options in settings: Never / Daily / Weekly / Manual only
  - Recommendation: Yes, add this flexibility

## Rollback Feature

- **Q: Should we support rollback to previous version if update breaks?**
  - Useful for: Failed updates, bugs in new version
  - Complexity: Medium (need to keep old version around)
  - Recommendation: Nice-to-have, not MVP

## Platform Priority

- **Q: Which platforms should we support first?**
  - All three (macOS, Windows, Linux) from the start?
  - Or start with macOS only (your current platform)?
  - Recommendation: Start with macOS, add others before 1.0

## Update Notification Urgency

- **Q: Should critical security updates be more prominent?**
  - Distinguish between: feature updates vs. security patches
  - Force update for critical security issues?
  - Recommendation: Add "priority" field to releases, handle differently

## Download Location

- **Q: Where should update artifacts be downloaded temporarily?**
  - Options:
    1. System temp directory
    2. `~/.tether/updates/`
    3. Platform-specific cache directory
  - Recommendation: Use Tauri's app cache directory (platform-specific, auto-cleaned)
