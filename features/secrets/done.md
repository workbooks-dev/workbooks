# Secrets - Completed

## Design & Documentation

- [x] Complete security design (see `/encryption.md`)
- [x] Feature specification and user flows
- [x] Integration points identified

## Backend Implementation (Rust)

### Encryption System
- [x] System keychain integration (macOS Keychain, cross-platform via `keyring` crate)
- [x] AES-256-GCM encryption for secret values
- [x] Per-project encryption keys stored in system keychain
- [x] Secure key generation and storage

### Storage
- [x] SQLite database for encrypted secrets (`.tether/secrets.db`)
- [x] Schema with id, key, encrypted_value, created_at, modified_at
- [x] Encryption/decryption functions
- [x] CRUD operations (create, read, update, delete)

### Tauri Commands
- [x] `add_secret(key, value)` - Add new secret
- [x] `get_secret(key)` - Retrieve decrypted secret
- [x] `list_secrets()` - List all secret keys (not values)
- [x] `update_secret(key, value)` - Update existing secret
- [x] `delete_secret(key)` - Delete secret
- [x] `get_all_secrets()` - Get all secrets with values (for injection)
- [x] `import_secrets_from_env(path)` - Import from .env file

### Runtime Injection
- [x] Secrets automatically injected as environment variables into kernel
- [x] Integration with engine startup (`start_engine_http`)
- [x] Integration with engine restart (`restart_engine_http`)
- [x] Secrets available via `os.environ["SECRET_NAME"]` in workbooks

## Frontend Implementation (React)

### Secrets Manager Component
- [x] Full-featured SecretsManager component (`src/components/SecretsManager.jsx`)
- [x] Table view of all secrets (shows key, created date, modified date)
- [x] Add secret dialog with password masking and visibility toggle
- [x] Edit secret functionality
- [x] Delete secret with confirmation
- [x] Search/filter functionality
- [x] Import from .env file
- [x] Polished UI with proper styling

### Sidebar Integration
- [x] Secrets section in sidebar with lock icon
- [x] Real-time secrets count display
- [x] Click to open secrets management tab
- [x] "Manage Secrets" button for quick access
- [x] Auto-refresh on secrets changes

### App Integration
- [x] Secrets tab support in App.jsx
- [x] Tab routing for secrets manager
- [x] Event system for secrets changes (`tether:secrets-changed`)

### WorkbookViewer Integration
- [x] Secrets indicator badge (lock icon with count)
- [x] Displays when secrets are active
- [x] Shows number of injected secrets
- [x] Auto-updates when secrets change

## Testing

- [x] Backend compiles successfully
- [x] Frontend builds without errors
- [x] All Tauri commands registered

## Notes

**Core secrets functionality is complete!** Users can:
1. Add, edit, and delete secrets via the UI
2. Import secrets from .env files
3. Secrets are encrypted and stored securely in system keychain
4. Secrets are automatically injected as environment variables into workbook kernels
5. Visual indicators show when secrets are active

**Not yet implemented** (see `todo.md`):
- Auto-detection of hardcoded secrets in cells
- **Output redaction (CRITICAL)** - Secrets currently leak into workbook outputs and logs
- Cell rewriting to use `os.environ`

## Authentication & Session Management

### Touch ID Session (Implemented)
- [x] 10-minute session timeout for read operations
- [x] Session state persisted in Tauri AppState across commands
- [x] Automatic re-authentication when session expires

### Authentication Policy
**No authentication required:**
- Listing secret keys (names only, not values)
- Adding new secrets (you're typing the value)
- Importing from .env files
- Getting all secrets for workbook environment injection (allows notebooks to start without prompts)

**Session-based authentication (10-minute timeout):**
- Viewing individual secret values (`get_secret`)
- First access triggers Touch ID, subsequent accesses within 10 minutes use session

**Always re-authenticate:**
- Updating existing secrets
- Deleting secrets

## Output Redaction & Warning System

### Frontend Secret Leakage Prevention (Implemented)
- [x] Tauri command `scan_outputs_for_secrets` to detect secrets in cell outputs
- [x] Scans all cell outputs before save for any secret values
- [x] SecretsWarningModal component with three action options
- [x] "Clear and Save" - Automatically clears outputs containing secrets, then saves
- [x] "Go Back and Fix" - Cancels save, allows user to manually fix
- [x] "Dangerously Save Anyway" - Requires explicit confirmation before saving with secrets
- [x] Modal shows list of affected cell indices
- [x] Integration with WorkbookViewer save workflow
- [x] Prevents accidental secret exposure in committed notebooks

**Security Flow:**
1. **Proactive scanning**: After any cell executes, outputs are automatically scanned for secret values
2. **Visual warning**: If secrets detected, Save button changes to amber "⚠ Save"
3. **User awareness**: Tooltip shows "Secrets detected in outputs - click to review"
4. **Modal on save**: Clicking the warning button shows modal with three options
5. **Safe by default**: Cannot save with secrets without explicit user choice

**Known Issues**:
- Session lock UI not yet implemented (lock button, status indicator)
