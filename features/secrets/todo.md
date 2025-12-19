# Secrets - To Do

## Critical Issues

### Output Redaction (IMPLEMENTED)
- [x] **Frontend warning when saving workbooks with secrets in output**
  - [x] Before save, scan all cell outputs for secret values
  - [x] If secrets detected, show modal with options:
    - [x] "Clear and Save" - Clear outputs containing secrets, then save
    - [x] "Go Back and Fix" - Cancel save, let user fix manually
    - [x] "Dangerously Save Anyway" - Save with secrets (requires confirmation)
  - [x] Show which cells contain secrets in the warning
  - [x] Modal highlights affected cell indices

- [ ] **Backend automatic redaction (alternative/complement to frontend warning)**
  - [ ] Scan outputs before save/display
  - [ ] Replace secret values with `[REDACTED]` marker
  - [ ] Log redactions for audit
  - [ ] Test that secrets never appear in saved notebook files
  - [ ] Verify redaction works for partial matches and variations

### Touch ID Session Management (IMPLEMENTED)
- [x] **Implement persistent Touch ID authentication session** - Working with 10-minute timeout
  - [x] Implement session state management in Rust backend
  - [x] Keep authentication session active for 10 minutes
  - [x] Only re-prompt for Touch ID after session timeout
  - [x] Allow reading secrets without re-authentication during active session
  - [x] Always require re-authentication for destructive operations (delete, edit)
  - [x] Session state persists across Tauri commands via AppState
  - [x] Store session timestamp and validate on each secrets operation
  - [ ] Session should expire on app close or manual lock
  - [ ] Visual indicator of authentication status (locked/unlocked state)
  - [ ] "Lock Secrets" button to manually invalidate session

## Future Enhancements

## Notebook Detection
- [ ] If `os.environ` is used, automatically check to see if it's in secrets. If it's not, then add it as a "unset value" with a row of items below the in-use ones.

## Auto-Detection

- [ ] Pattern recognition
  - [ ] Regex patterns for common secret formats
  - [ ] Entropy analysis for random strings
  - [ ] Whitelist of known secret key names
  - [ ] Configurable detection sensitivity

- [ ] User prompts
  - [ ] "Detected secret" dialog
  - [ ] Suggest key name based on variable
  - [ ] Partially mask detected value
  - [ ] One-click migration to secrets manager

- [ ] Cell rewriting
  - [ ] Replace hardcoded value with `os.environ["KEY"]`
  - [ ] Update cell source
  - [ ] Preserve code structure
  - [ ] Add import os if needed

## Output Redaction

- [ ] Scan outputs before save
  - [ ] Text outputs (stdout, stderr)
  - [ ] HTML content
  - [ ] Error messages
  - [ ] Return values

- [ ] Replace with redaction marker
  - [ ] `[REDACTED]` or similar
  - [ ] Preserve output structure
  - [ ] Log redactions for audit

## Migration & Import

- [ ] .env file detection
  - [ ] Scan for .env in project root
  - [ ] Parse .env format
  - [ ] Offer to import

- [ ] Import flow
  - [ ] Preview secrets to import
  - [ ] Encrypt and store
  - [ ] Option to delete original .env
  - [ ] Update code references

- [ ] External edit detection
  - [ ] File watcher for .env
  - [ ] Prompt on changes
  - [ ] Suggest migration

## Testing & Polish

- [ ] Test secrets workflow on Windows (keyring integration)
- [ ] Test secrets workflow on Linux (keyring integration)
- [ ] Security audit of encryption implementation
- [ ] User documentation for secrets workflow
- [ ] Tutorial/onboarding for first-time secrets users
