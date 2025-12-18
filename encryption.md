# Environment Variable Encryption

## Overview

Encrypt environment variables by default for each Tether project. Use system-level authentication (Touch ID/password) to decrypt values when needed. Encryption is per-project with keys stored in the system keychain.

## File Structure

```
my-project/
├── .env.tether          # Encrypted env vars (git-safe to commit)
├── .env                 # Plain text (if user has legacy, optional)
├── .tether/
│   └── env.hash         # Hash of .env.tether to detect external edits
```

## Encryption Strategy

**Encryption Key Storage:**
- Per-project key stored in system keychain
- Keychain entry name: `tether.{absolute_project_path_hash}.env_key`
- First time setup prompts for system auth (Touch ID on macOS, password elsewhere)
- Key is generated automatically on first env var save

**Encryption Algorithm:**
- AES-256-GCM (authenticated encryption)
- Each value encrypted separately with project key
- File format: JSON with encrypted values

**Example `.env.tether` format:**
```json
{
  "version": 1,
  "encrypted": true,
  "vars": {
    "DATABASE_URL": "encrypted_base64_string_here",
    "API_KEY": "encrypted_base64_string_here"
  }
}
```

## User Flows

### 1. Initial Setup (Fresh Project)
- User clicks "Manage Environment Variables" button in project settings
- Adds key-value pairs in UI (plain text in form)
- On save:
  - Prompts for system authentication
  - Generates encryption key and stores in keychain
  - Encrypts each value
  - Writes `.env.tether`
  - Stores hash in `.tether/env.hash`

### 2. Migration from `.env`
- Tether detects plain `.env` on project open
- Prompt dialog:
  ```
  Found .env file with plain text environment variables.

  Would you like to encrypt them with Tether?

  [Encrypt to .env.tether] [Keep as .env] [Cancel]

  Note: After encrypting, you can safely delete .env
  ```
- If "Encrypt to .env.tether":
  - Parse `.env` file
  - Prompt for system authentication
  - Encrypt and write `.env.tether`
  - Ask if user wants to delete original `.env`

### 3. External Edit Detection
- On project open or kernel start, compute hash of `.env.tether`
- Compare with stored hash in `.tether/env.hash`
- If mismatch, prompt:
  ```
  .env.tether was modified outside of Tether.

  What would you like to do?

  [Re-encrypt with Tether] - Treat as new plain text values and re-encrypt
  [Move to .env] - Convert to plain text .env file
  [Discard changes] - Revert to last known good state
  [Cancel]
  ```

### 4. Editing Existing Vars
- User opens "Manage Environment Variables"
- Prompts for system authentication
- Decrypts all values and shows in UI
- User edits
- On save, re-encrypts and updates `.env.tether` and hash

### 5. Kernel Injection
- When starting Jupyter engine/kernel:
  1. Check if `.env.tether` exists
  2. Prompt for system authentication (once per session)
  3. Decrypt all values
  4. Inject into kernel environment before Python starts
  5. Cache decrypted values in memory for session
- Now `os.environ`, `load_dotenv()`, and any third-party packages work transparently

## Implementation Plan

### Phase 1: Rust Backend

**New file: `src-tauri/src/encryption.rs`**
- `generate_key()` - Generate AES-256 key
- `encrypt_value(value: &str, key: &[u8]) -> Result<String>`
- `decrypt_value(encrypted: &str, key: &[u8]) -> Result<String>`
- Struct `EncryptedEnvFile` with serialize/deserialize

**New file: `src-tauri/src/keychain.rs`**
- Platform-specific keychain integration:
  - macOS: `security-framework` crate
  - Windows: `windows` crate (Credential Manager or DPAPI)
  - Linux: `secret-service` crate (libsecret)
- `store_key(project_path: &str, key: &[u8]) -> Result<()>`
- `retrieve_key(project_path: &str) -> Result<Vec<u8>>`
- `delete_key(project_path: &str) -> Result<()>`

**Update `src-tauri/src/lib.rs`** - Add Tauri commands:
- `get_env_vars()` - Decrypt and return all env vars
- `set_env_vars(vars: HashMap<String, String>)` - Encrypt and save
- `import_from_env()` - Read plain .env and return for encryption
- `detect_env_changes()` - Check for external modifications
- `resolve_env_conflict(action: String)` - Handle dirty state resolution

### Phase 2: Python Engine Integration

**Update `src-tauri/engine_server.py`:**
- Add `env_vars` parameter to `start_engine()` endpoint
- Inject env vars into kernel environment before starting:
  ```python
  import os
  for key, value in env_vars.items():
      os.environ[key] = value
  ```

**Update `src-tauri/src/engine_http.rs`:**
- Before calling `start_engine()`, decrypt `.env.tether` if exists
- Pass decrypted vars to engine server
- Cache decrypted values for session (clear on project close)

### Phase 3: React Frontend

**New component: `src/components/EnvManager.jsx`**
- Table of key-value pairs (editable)
- Add/delete rows
- Save/cancel buttons
- Show encryption status indicator
- Import from .env button
- Export plain text warning

**Update `src/App.jsx`:**
- Add "Environment Variables" button to project view
- Modal/sidebar for EnvManager
- Handle conflict resolution dialogs

**New component: `src/components/EnvConflictDialog.jsx`**
- Shows options when external changes detected
- Radio buttons for actions
- Explanation text for each option

## Security Considerations

1. **Key Storage**: System keychain is the most secure option. Never store encryption keys in plain text.

2. **Session Caching**: Decrypted values stay in memory only. Cleared on project close or app quit.

3. **Git Safety**: `.env.tether` is safe to commit. Without the keychain entry, values cannot be decrypted.

4. **Sharing Projects**:
   - Encrypted `.env.tether` can be safely shared
   - Recipient won't have the decryption key
   - They'll need to create their own .env or re-encrypt with their own key

5. **Key Rotation**:
   - Future feature: "Re-encrypt with new key"
   - Useful if key is compromised or when moving to new machine

## Future Enhancements

- **Team Sharing**: Encrypt with shared secret or asymmetric keys
- **Selective Encryption**: Mark certain vars as "plain text OK"
- **Environment Profiles**: dev/staging/prod env var sets
- **Audit Log**: Track who accessed/modified env vars when
- **Cloud Sync**: Encrypted cloud backup of env vars

## Testing Plan

1. **Unit Tests (Rust)**:
   - Encryption/decryption round-trip
   - Keychain storage/retrieval
   - Hash detection

2. **Integration Tests**:
   - Full flow: add var → encrypt → restart kernel → access in Python
   - Migration from plain .env
   - External edit detection

3. **Platform Tests**:
   - Verify keychain integration on macOS, Windows, Linux
   - Test system auth prompts

4. **Edge Cases**:
   - Missing keychain entry (deleted manually)
   - Corrupted .env.tether file
   - Permission denied on keychain
   - Multiple Tether instances accessing same project
