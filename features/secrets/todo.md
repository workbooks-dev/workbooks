# Secrets - To Do

## Critical Issues

- [ ] **Backend automatic redaction (alternative/complement to frontend warning)**
  - [ ] Scan outputs before save/display
  - [ ] Replace secret values with `[REDACTED]` marker
  - [ ] Log redactions for audit
  - [ ] Test that secrets never appear in saved notebook files
  - [ ] Verify redaction works for partial matches and variations

### Touch ID Session Management

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
