# Secrets - To Do

## High Priority

### Touch ID Session Management UI

- [ ] Visual indicator of authentication status (locked/unlocked state)
- [ ] "Lock Secrets" button to manually invalidate session
- [ ] Session should expire on app close (currently persists until timeout)

## Medium Priority

### Auto-Detection of Hardcoded Secrets (Pre-Execution)

**Goal:** Detect secrets BEFORE execution and prompt user to move them to secrets manager

- [ ] Pattern recognition
  - [ ] Regex patterns for common secret formats (API keys, tokens, passwords)
  - [ ] Entropy analysis for random strings
  - [ ] Whitelist of known secret key names
  - [ ] Configurable detection sensitivity

- [ ] User prompts
  - [ ] "Detected secret" dialog before cell execution
  - [ ] Suggest key name based on variable
  - [ ] Partially mask detected value
  - [ ] One-click migration to secrets manager

- [ ] Cell rewriting
  - [ ] Replace hardcoded value with `os.environ["KEY"]`
  - [ ] Update cell source automatically
  - [ ] Preserve code structure
  - [ ] Add `import os` if needed

### Notebook Environment Detection

- [ ] Detect `os.environ` usage in code
- [ ] Check if referenced environment variable exists in secrets
- [ ] Show "unset value" warning row in secrets UI for missing secrets
- [ ] Quick-add button to create the missing secret

## Low Priority

### .env File Integration Enhancements

**Note:** Basic .env import is already implemented. These are enhancements:

- [ ] Automatic .env file detection
  - [ ] Scan for .env in project root on project open
  - [ ] Parse .env format
  - [ ] Offer to import if found

- [ ] Import flow improvements
  - [ ] Preview secrets to import
  - [ ] Option to delete original .env after import
  - [ ] Update code references to use `os.environ`

- [ ] External edit detection
  - [ ] File watcher for .env changes
  - [ ] Prompt on changes
  - [ ] Suggest migration

### Backend Automatic Redaction (Optional Enhancement)

**Note:** Output redaction is currently handled via frontend warning system (SecretsWarningModal). This would be a backend alternative/complement.

- [ ] Backend scan outputs before save/display
  - [ ] Replace secret values with `[REDACTED]` marker automatically
  - [ ] Log redactions for audit trail
  - [ ] Test that secrets never appear in saved notebook files
  - [ ] Verify redaction works for partial matches and variations

## Testing & Documentation

- [ ] Test secrets workflow on Windows (keyring integration)
- [ ] Test secrets workflow on Linux (keyring integration)
- [ ] Security audit of encryption implementation
- [ ] User documentation for secrets workflow
- [ ] Tutorial/onboarding for first-time secrets users
- [ ] Test session expiration on app close/restart
