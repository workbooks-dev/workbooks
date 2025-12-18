# Environment Variable Encryption

## Overview

Tether provides comprehensive secrets management designed for non-technical users:

1. **Auto-Detection**: Automatically detect hardcoded secrets in notebook cells (API keys, passwords, tokens)
2. **Guided Security**: Prompt users to securely store secrets with one click
3. **Code Rewriting**: Automatically rewrite cells to use `os.environ` instead of hardcoded values
4. **Encryption**: Encrypt secrets using system keychain (Touch ID on macOS)
5. **Output Redaction**: Automatically redact secret values from notebook outputs before saving
6. **Git-Safe**: Encrypted `.env.tether` and redacted outputs are safe to commit
7. **Shareable**: Team members use their own secrets for the same project

**Key Principles:**
- Zero configuration for users - it just works
- Educational, not annoying - teach best practices gently
- Security by default - secrets never leak into git
- Non-technical friendly - use "Secrets" terminology, not ".env" or "environment variables"

## File Structure

**Project Directory (shareable):**
```
my-project/
├── .env.tether          # Encrypted env vars (safe to share/commit)
├── .env                 # Plain text (if user has legacy, optional)
```

**User-Specific Storage (per-machine):**
```
~/.tether/projects/<project-name>-<hash>/
├── env.hash             # Hash of .env.tether to detect external edits
└── env.key              # Encryption key (backup, also in system keychain)
```

**Why This Design:**
- **Shareable by default**: Project folder stays clean (no user-specific files), safe to commit to git
- **Per-user secrets**: Each person has their own API keys/credentials for the same project
- **No .gitignore needed**: `.env.tether` is encrypted, safe to commit. User secrets in `~/.tether/` never touch git
- **Non-technical friendly**: Users don't need to understand ".env", encryption, or gitignore
- **Stable hash**: `<hash>` is derived from absolute project path (e.g., `/Users/jmitch/Dev/my-project` → `a3f7b9c2`)
  - Same project, same machine → same hash
  - Different machine or different clone location → different hash (correct behavior)
  - Prevents secrets leaking between different clones of the same repo

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
  - Writes `.env.tether` to project directory
  - Stores hash in `~/.tether/projects/<project-name>-<hash>/env.hash`

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
- Compare with stored hash in `~/.tether/projects/<project-name>-<hash>/env.hash`
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

### 5. Secret Detection in Code Cells

**Pattern Detection:**
When a user runs a cell, scan the source code for common patterns that indicate hardcoded secrets:
- `os.environ["KEY"] = "value"`
- `api_key = "sk-..."`
- `password = "..."`
- Any string matching common secret patterns (API keys, tokens, passwords)

**Auto-Detection Triggers:**
- Regex patterns for common API key formats:
  - OpenAI: `sk-[a-zA-Z0-9]{32,}`
  - AWS: `AKIA[A-Z0-9]{16}`
  - GitHub: `ghp_[a-zA-Z0-9]{36}`
  - Stripe: `sk_live_[a-zA-Z0-9]{24,}`
  - Any `password`, `token`, `secret`, `api_key` variable assignments
- Variable names containing: `key`, `token`, `password`, `secret`, `api`, `auth`

**User Flow:**
1. User runs cell containing:
   ```python
   import openai
   openai.api_key = "sk-abc123def456..."
   ```
2. Tether detects hardcoded secret and shows dialog:
   ```
   🔒 Hardcoded secret detected

   Found: "sk-abc123..." in your code

   Would you like to:
   • Save to Secrets as "OPENAI_API_KEY" and update cell
   • Ignore this time
   • Don't warn me again for this notebook

   [Save & Update] [Ignore] [Cancel]
   ```
3. If "Save & Update":
   - Prompts for Touch ID to unlock secrets
   - Opens quick dialog: "Save as: [OPENAI_API_KEY]" with suggested name
   - Saves secret to `.env.tether`
   - **Automatically rewrites cell** to:
     ```python
     import openai
     import os
     openai.api_key = os.environ["OPENAI_API_KEY"]
     ```
   - Re-runs cell with new code
   - Shows success message: "Secret saved and cell updated"

**Implementation:**
- Add `detect_secrets_in_code(source: str) -> Vec<DetectedSecret>` in Rust
- `DetectedSecret` struct contains: value, suggested_name, line_number, pattern_type
- Frontend shows dialog before cell execution completes
- After user confirms, use Edit tool pattern to rewrite cell source
- Update notebook JSON in memory and on disk

**False Positive Prevention:**
- Don't detect strings shorter than 8 characters (unlikely to be real secrets)
- Ignore common test values: "test", "example", "demo", "placeholder"
- Ignore if already using `os.environ`, `os.getenv`, or `load_dotenv()`
- Ignore strings in comments or docstrings
- Use heuristics: API keys usually have specific prefixes (sk-, ghp-, AKIA, etc.)
- Allow user to permanently ignore specific patterns via settings

**Educational Value:**
- First time detection shows expanded help text:
  ```
  💡 Tip: Hardcoding secrets in code is risky!

  Tether can securely store this secret and automatically
  inject it when your code runs. The secret will be:
  • Encrypted on your machine
  • Protected by Touch ID
  • Never saved in the notebook file
  • Redacted from outputs

  [Learn More] [Save & Update] [Ignore]
  ```
- Teaches users the right pattern without being preachy
- "Learn More" links to docs about why secrets management matters

**Code Rewriting Examples:**

| Before (Hardcoded) | After (Secure) | Secret Name |
|-------------------|----------------|-------------|
| `openai.api_key = "sk-abc123"` | `import os`<br>`openai.api_key = os.environ["OPENAI_API_KEY"]` | OPENAI_API_KEY |
| `client = Anthropic(api_key="sk-ant-xyz")` | `import os`<br>`client = Anthropic(api_key=os.environ["ANTHROPIC_API_KEY"])` | ANTHROPIC_API_KEY |
| `password = "mypassword123"` | `import os`<br>`password = os.environ["PASSWORD"]` | PASSWORD |
| `AWS_SECRET = "AKIAIOSFODNN7"` | `import os`<br>`AWS_SECRET = os.environ["AWS_SECRET"]` | AWS_SECRET |
| `db_url = "postgres://user:pass@host"` | `import os`<br>`db_url = os.environ["DATABASE_URL"]` | DATABASE_URL |
| `headers = {"Authorization": "Bearer abc123"}` | `import os`<br>`headers = {"Authorization": f"Bearer {os.environ['API_TOKEN']}"}` | API_TOKEN |

**Smart Context Detection:**
- `import openai` + `"sk-..."` → OPENAI_API_KEY
- `from anthropic import Anthropic` + `"sk-ant-..."` → ANTHROPIC_API_KEY
- `import stripe` + `"sk_live..."` → STRIPE_API_KEY
- Variable name `github_token` → GITHUB_TOKEN
- Connection string pattern → DATABASE_URL
- Bearer token pattern → API_TOKEN or BEARER_TOKEN

### 6. Kernel Injection + Output Redaction
- When starting Jupyter engine/kernel:
  1. Check if `.env.tether` exists
  2. Prompt for system authentication (once per session)
  3. Decrypt all values
  4. Inject into kernel environment before Python starts
  5. **Keep track of secret values for output scanning**
  6. Cache decrypted values in memory for session
- Now `os.environ`, `load_dotenv()`, and any third-party packages work transparently

**Output Redaction (Security Critical):**
- **During live execution**: User sees real output values (helps with debugging)
- **When saving to disk**: All outputs are scanned and secret values are replaced with `[REDACTED: SECRET_NAME]`
- Example:
  ```python
  # User accidentally prints their API key
  print(os.environ["OPENAI_API_KEY"])
  ```
  **User sees in live UI:** `sk-abc123def456...`

  **Saved to notebook file (.ipynb):**
  ```
  [REDACTED: OPENAI_API_KEY]
  ```
- **Redaction scope:**
  - All text outputs: stdout, stderr, execute_result, display_data
  - HTML outputs (DataFrames rendered as tables)
  - JSON outputs
  - Error tracebacks
  - Partial matches (if secret is "sk-abc123", redact "my-key-is-sk-abc123" → "my-key-is-[REDACTED: OPENAI_API_KEY]")
- **Implementation point**: Redaction happens in `save_workbook()` Tauri command, not in engine server
- Prevents secrets from being committed in notebook outputs or shared accidentally

## Implementation Plan

### Phase 1: Rust Backend

**New file: `src-tauri/src/encryption.rs`**
- `generate_key()` - Generate AES-256 key
- `encrypt_value(value: &str, key: &[u8]) -> Result<String>`
- `decrypt_value(encrypted: &str, key: &[u8]) -> Result<String>`
- `compute_project_hash(project_path: &Path) -> String` - SHA256 hash of absolute project path (first 16 chars)
- `get_user_secrets_dir(project_path: &Path) -> PathBuf` - Returns `~/.tether/projects/<project-name>-<hash>/`
- `compute_file_hash(file_path: &Path) -> Result<String>` - SHA256 hash of file contents for change detection
- Struct `EncryptedEnvFile` with serialize/deserialize

**New file: `src-tauri/src/keychain.rs`**
- Platform-specific keychain integration:
  - macOS: `security-framework` crate
  - Windows: `windows` crate (Credential Manager or DPAPI)
  - Linux: `secret-service` crate (libsecret)
- `store_key(project_path: &str, key: &[u8]) -> Result<()>`
- `retrieve_key(project_path: &str) -> Result<Vec<u8>>`
- `delete_key(project_path: &str) -> Result<()>`

**New file: `src-tauri/src/secret_detection.rs`**
- `detect_secrets_in_code(source: &str) -> Vec<DetectedSecret>`
- `DetectedSecret` struct:
  ```rust
  pub struct DetectedSecret {
      pub value: String,           // The actual secret value
      pub suggested_name: String,  // e.g., "OPENAI_API_KEY"
      pub line_number: usize,
      pub pattern_type: String,    // "openai_key", "aws_key", "generic_password", etc.
      pub context: String,         // Surrounding code for display
  }
  ```
- Regex patterns for common secret formats
- Smart name suggestion based on context (variable names, imports)
- `rewrite_cell_to_use_env(source: &str, secret_name: &str, secret_value: &str) -> String`
  - Rewrites cell to replace hardcoded value with `os.environ["SECRET_NAME"]`
  - Adds `import os` if not present
  - Handles various assignment patterns

**Update `src-tauri/src/lib.rs`** - Add Tauri commands:
- `get_secrets()` - Decrypt and return all secrets (was `get_env_vars`)
- `set_secrets(vars: HashMap<String, String>)` - Encrypt and save to `.env.tether`, update hash in `~/.tether/projects/<project-name>-<hash>/env.hash`
- `add_secret(name: String, value: String)` - Add a single secret (used by detection feature)
- `import_from_env()` - Read plain .env and return for encryption (migration only)
- `detect_secrets_changes()` - Check if `.env.tether` hash matches stored hash
- `resolve_secrets_conflict(action: String)` - Handle external edit scenarios
- `has_secrets_configured()` - Check if user has set up secrets for this project (checks keychain)
- **`detect_secrets_in_code(source: String) -> Vec<DetectedSecret>`** - Scan cell source for hardcoded secrets
- **`rewrite_cell_source(source: String, secret_name: String, secret_value: String) -> String`** - Rewrite cell to use os.environ

### Phase 2: Python Engine Integration

**Update `src-tauri/engine_server.py`:**
- Add `env_vars` parameter to `start_engine()` endpoint
- Inject env vars into kernel environment before starting:
  ```python
  import os
  for key, value in env_vars.items():
      os.environ[key] = value
  ```
- No redaction at engine level (user sees real values during execution)

**Update `src-tauri/src/engine_http.rs`:**
- Before calling `start_engine()`, decrypt `.env.tether` if exists
- Pass decrypted vars to engine server
- Cache decrypted values for session (clear on project close)

**Update `src-tauri/src/fs.rs`:**
- **Modify `save_workbook()` command to apply output redaction:**
  ```rust
  pub async fn save_workbook(
      workbook_path: String,
      content: serde_json::Value,
      state: State<'_, AppState>,
  ) -> Result<(), String> {
      // Get current secrets for this project
      let secrets = get_current_secrets(&state)?;

      // Deep-scan notebook JSON for outputs
      let redacted_content = redact_notebook_outputs(content, &secrets)?;

      // Save redacted version to disk
      std::fs::write(&workbook_path, serde_json::to_string_pretty(&redacted_content)?)?;
      Ok(())
  }

  fn redact_notebook_outputs(
      mut notebook: serde_json::Value,
      secrets: &HashMap<String, String>,
  ) -> Result<serde_json::Value> {
      // Iterate through cells
      // For each output in cell.outputs:
      //   - Scan text/plain, text/html, application/json
      //   - Replace secret values with [REDACTED: SECRET_NAME]
      // Return modified notebook
  }
  ```
- Redaction is transparent to user (they see real values in UI, but file contains redacted)

### Phase 3: React Frontend

**New component: `src/components/SecretsManager.jsx`** (was EnvManager)
- Simple title: "Secrets" (not "Environment Variables")
- Table of key-value pairs (editable)
- Add/delete rows
- Save/cancel buttons
- Green lock icon when encrypted
- Import from .env button (for migration only, hidden unless .env exists)
- No export option (too risky for non-technical users)

**Update `src/App.jsx`:**
- Add "Secrets" button to project toolbar (lock icon)
- Modal/sidebar for SecretsManager
- Handle conflict resolution dialogs
- Show Touch ID prompt when accessing secrets

**Update `src/components/WorkbookViewer.jsx`:**
- Add small lock icon/badge near kernel status when secrets are active
- Tooltip: "Secrets are protected - outputs will be redacted when saved"
- Auto-save already calls `save_workbook()`, which applies redaction automatically
- User sees real values in output areas, but file is saved with redacted values
- Optional: Add a one-time info banner explaining redaction when secrets are first used
- **Secret detection integration:**
  - Before executing cell, call `detect_secrets_in_code(cell.source)`
  - If secrets found, show detection dialog
  - On "Save & Update": save secret, rewrite cell source, re-execute cell
  - On "Ignore": proceed with execution normally
  - Track "don't warn again" preference per notebook in local storage

**New component: `src/components/SecretDetectionDialog.jsx`**
- Modal dialog shown when hardcoded secrets are detected
- Shows detected secret (partially masked: "sk-abc...456")
- Suggested secret name (editable text field)
- Actions: Save & Update, Ignore, Cancel
- Checkbox: "Don't warn me again for this notebook"

**New component: `src/components/EnvConflictDialog.jsx`**
- Shows options when external changes detected
- Radio buttons for actions
- Explanation text for each option

## Complete User Journey Example

**Scenario: New user adds OpenAI API key**

1. **User writes code:**
   ```python
   import openai
   openai.api_key = "sk-proj-abc123def456..."
   ```

2. **User hits Shift+Enter to run cell**

3. **Tether detects hardcoded secret, shows dialog:**
   ```
   💡 Hardcoded secret detected

   Found: "sk-proj-abc..." in your code

   Tether can securely store this and protect it:
   • Encrypted on your machine with Touch ID
   • Never saved in notebook file
   • Automatically redacted from outputs

   Save as: [OPENAI_API_KEY]

   [Save & Update] [Ignore] [Cancel]
   ```

4. **User clicks "Save & Update"**
   - Touch ID prompt appears
   - Secret saved to `~/.tether/projects/<project>-<hash>/`
   - `.env.tether` created in project folder (encrypted)
   - Cell is automatically rewritten to:
     ```python
     import openai
     import os
     openai.api_key = os.environ["OPENAI_API_KEY"]
     ```
   - Cell re-executes with new code
   - Green lock icon appears near kernel status

5. **User continues working:**
   - Accidentally prints the key: `print(openai.api_key)`
   - Live UI shows: `sk-proj-abc123...` (helpful for debugging)
   - Auto-save triggers
   - **Notebook file saved with redacted output:** `[REDACTED: OPENAI_API_KEY]`

6. **User commits to git:**
   - `.env.tether` (encrypted) ✅ Safe to commit
   - Notebook with `[REDACTED: OPENAI_API_KEY]` outputs ✅ Safe to commit
   - No secrets exposed!

7. **Teammate clones repo:**
   - Opens project in Tether
   - Tether detects `.env.tether` but no decryption key
   - Prompts: "This project uses secrets. Set up your own?"
   - Teammate adds their own OpenAI key
   - Saved to their `~/.tether/` directory
   - Everything works, different keys per person

## Non-Technical User Experience

Most Tether users won't understand `.env` files or encryption. The UI should hide this complexity:

**What users see:**
- "Secrets" button in project toolbar (not "Environment Variables")
- Simple table: "Name" and "Value" columns
- Examples shown: "OPENAI_API_KEY", "DATABASE_PASSWORD"
- Green lock icon when encrypted
- No mention of .env, keychain, or hashing

**What happens behind the scenes:**
- First time adding a secret:
  - "Tether needs to unlock your secrets" → Touch ID prompt
  - Values encrypted and saved to `.env.tether`
  - Hash stored in `~/.tether/projects/<project-name>-<hash>/env.hash`
  - Key stored in system keychain
- Opening a shared project with `.env.tether`:
  - "This project uses secrets. Set up your own?" → Yes/No
  - If Yes: Opens secrets editor, saves to their own `~/.tether` location
  - If No: Project works without secrets (Python code may error if it needs them)

**Key principle:** Never show file paths, encryption details, or technical jargon. Just "secrets" and a lock icon.

**When opening a notebook with redacted outputs:**
- User sees `[REDACTED: OPENAI_API_KEY]` in previous cell outputs
- When they re-run those cells, real values appear in live UI
- On next save, outputs are redacted again
- This is expected behavior and reinforces that secrets are protected

## Security Considerations

1. **Key Storage**: System keychain is the most secure option. Never store encryption keys in plain text.

2. **Session Caching**: Decrypted values stay in memory only. Cleared on project close or app quit.

3. **Git Safety**: `.env.tether` is safe to commit. Without the keychain entry, values cannot be decrypted.

3.5. **Output Redaction**:
   - All cell outputs are scanned for secret values when saving the notebook to disk
   - Prevents accidental exposure through `print()`, logging, error messages, DataFrames, etc.
   - **Two-stage approach**:
     1. Live execution: User sees real values (helpful for debugging)
     2. Save to disk: `save_workbook()` applies redaction before writing .ipynb file
   - Redacted format: `[REDACTED: SECRET_NAME]` so user knows which secret was used
   - **Git safety**: Even if user commits notebook, outputs contain no actual secret values
   - **Edge cases handled**:
     - Partial matches (secrets embedded in longer strings)
     - Rich outputs (HTML tables, JSON, error tracebacks)
     - Binary outputs (images/plots) are left untouched
   - **Trade-off**: Slight performance cost on save, but critical for security

4. **Sharing Projects**:
   - Project folder (with `.env.tether`) can be safely shared via git, Dropbox, etc.
   - Recipient won't have the encryption key (it's in your `~/.tether` and keychain)
   - When recipient opens project:
     - Tether detects `.env.tether` but no key in their keychain
     - Prompts: "This project has encrypted variables. Set up your own secrets?"
     - User creates their own variables (may be different from yours)
     - Their secrets stored in their own `~/.tether/projects/<project-name>-<hash>/`
   - **Key insight**: Same project, different secrets per user. Perfect for teams where everyone has their own API keys/credentials.

5. **Key Rotation**:
   - Future feature: "Re-encrypt with new key"
   - Useful if key is compromised or when moving to new machine

## Implementation Quick Reference

**Flow: Detecting and storing a secret**
```
User runs cell
  ↓
WorkbookViewer calls detect_secrets_in_code(cell.source)
  ↓
If secrets found → Show SecretDetectionDialog
  ↓
User clicks "Save & Update"
  ↓
Touch ID prompt → Unlock keychain
  ↓
Call add_secret(name, value) → Encrypt & save to .env.tether
  ↓
Call rewrite_cell_source() → Replace hardcoded value with os.environ
  ↓
Update cell in notebook JSON
  ↓
Re-execute cell with new code
  ↓
Show success message + lock icon
```

**Flow: Redacting outputs on save**
```
User triggers save (auto-save, Cmd+S, on-blur)
  ↓
WorkbookViewer calls save_workbook(workbook_path, content)
  ↓
Rust: Load current secrets from .env.tether
  ↓
Rust: Deep-scan notebook JSON for all outputs
  ↓
For each text output:
  - Replace secret values with [REDACTED: SECRET_NAME]
  ↓
Write redacted notebook to disk
  ↓
User still sees real values in UI (no change to state)
```

**Flow: Opening a shared project**
```
User opens project with .env.tether
  ↓
Check keychain for decryption key
  ↓
If no key found → Show dialog:
  "This project uses secrets. Set up your own?"
  ↓
If Yes → Open SecretsManager
  ↓
User adds their own secrets
  ↓
Save to their ~/.tether/projects/<project>-<hash>/
  ↓
Project works with their secrets
```

## Future Enhancements

- **Team Sharing**: Encrypt with shared secret or asymmetric keys
- **Secret Rotation**: Auto-detect expired API keys and prompt renewal
- **Selective Encryption**: Mark certain vars as "plain text OK"
- **Environment Profiles**: dev/staging/prod env var sets
- **Audit Log**: Track who accessed/modified env vars when
- **Cloud Sync**: Encrypted cloud backup of env vars
- **Smart Detection Improvements**: Learn from user corrections to improve detection accuracy
- **Integration with 1Password/Bitwarden**: Import secrets from password managers

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

5. **Secret Detection Tests**:
   - **Pattern matching**:
     - Detect OpenAI keys: `api_key = "sk-proj-abc123..."`
     - Detect AWS keys: `aws_key = "AKIAIOSFODNN7EXAMPLE"`
     - Detect GitHub tokens: `token = "ghp_abc123..."`
     - Detect generic passwords: `password = "my_secret_pass"`
     - Variable names: `API_KEY`, `api_token`, `secret`, `auth_token`
   - **Smart naming**:
     - `openai.api_key = "..."` → suggests "OPENAI_API_KEY"
     - `stripe.api_key = "..."` → suggests "STRIPE_API_KEY"
     - `password = "..."` → suggests "PASSWORD"
     - `MY_TOKEN = "..."` → suggests "MY_TOKEN" (preserve user's naming)
   - **Cell rewriting**:
     - Simple assignment: `key = "value"` → `key = os.environ["KEY"]`
     - Nested assignment: `client.api_key = "value"` → `client.api_key = os.environ["API_KEY"]`
     - Multiple secrets in one cell → rewrite all
     - Preserve formatting and comments
     - Add `import os` only if not present
   - **Edge cases**:
     - False positives (long random strings that aren't secrets)
     - Secrets in multi-line strings
     - Secrets in f-strings
     - Already using os.environ (don't re-detect)
   - **User flow**:
     - Verify dialog appears before execution
     - Verify cell is rewritten correctly on "Save & Update"
     - Verify cell executes successfully after rewrite
     - Verify "don't warn again" preference is saved

6. **Redaction Tests (Security Critical)**:
   - **Basic redaction**:
     - `print(os.environ["SECRET"])` → verify output is `[REDACTED: SECRET]...ENDING XYZ`
     - Verify live UI shows real value, but .ipynb file contains redacted
   - **Partial matches**:
     - `print(f"My key is {os.environ['SECRET']}")` → `My key is [REDACTED: SECRET]...ENDING XYZ`
   - **Multiple secrets**:
     - Print multiple secrets in one output → all redacted correctly
   - **Rich outputs**:
     - DataFrame containing secret value → HTML output redacted
     - JSON output containing secret → redacted
     - Error traceback containing secret → redacted
   - **Binary outputs**:
     - Images/plots remain untouched (no text to redact)
   - **Edge case - secret in secret**:
     - If one secret value contains another, redact longest match first
   - **Performance**:
     - Large outputs (MB of text) should redact in reasonable time (<100ms)
   - **Auto-save integration**:
     - Verify auto-save triggers redaction
     - Manual save triggers redaction
     - All save paths apply redaction consistently
