# Secrets Management

## Overview

Workbooks provides secure storage for API keys, passwords, and other sensitive credentials. Secrets are encrypted and never exposed in code or outputs.

**See `/encryption.md` for complete security implementation details.**

## Design Philosophy

**Problems Solved:**
1. No more hardcoded API keys in notebooks
2. No accidental commits of credentials to Git
3. No secrets in notebook outputs/logs
4. Secure sharing of notebooks (recipients add their own secrets)

**Security Approach:**
- All secrets encrypted using system keychain
- Touch ID authentication on macOS
- Values never written to disk unencrypted
- Auto-detection prevents hardcoding
- Output redaction prevents leakage

## User Experience

### Secrets Sidebar Section

Shows list of secret keys (values hidden):
- Lock icon header
- Secret names displayed
- Click header → Opens full secrets management tab
- Quick overview of what secrets are configured

### Secrets Management Tab

Full CRUD interface:
- Table of all secrets (name, last modified)
- "+ Add Secret" button
- Edit/delete buttons per secret
- Search/filter for large lists

**Add Secret Flow:**
1. Click "+ Add Secret"
2. Enter key name (e.g., `OPENAI_API_KEY`)
3. Enter value
4. Authenticate with Touch ID (macOS)
5. Secret encrypted and stored

### In Workbooks

**Auto-Detection:**
When a cell contains hardcoded secrets (detected by patterns):
1. Show warning dialog before execution
2. Offer to move secret to secrets manager
3. Auto-rewrite cell to use `os.environ["SECRET_NAME"]`
4. Execute with environment variable

**Example:**
```python
# Before (detected as secret)
api_key = "sk-abc123xyz..."

# After (auto-rewritten)
api_key = os.environ["OPENAI_API_KEY"]
```

**Lock Icon:**
- Shows in WorkbookViewer when secrets are active
- Indicates secure execution environment
- Lists which secrets are available

## Technical Implementation

### Storage

**Encrypted Storage:**
- `.workbooks/secrets.db` - SQLite database with encrypted values
- System keychain integration for encryption keys
- Per-project secrets scope

**Schema:**
```sql
CREATE TABLE secrets (
  id TEXT PRIMARY KEY,
  key TEXT UNIQUE NOT NULL,
  encrypted_value BLOB NOT NULL,
  created_at INTEGER NOT NULL,
  modified_at INTEGER NOT NULL
);
```

### Runtime Injection

**Environment Variables:**
- Secrets injected as environment variables into kernel
- Available via `os.environ["SECRET_NAME"]`
- Scoped to workbook execution only
- Not visible in process list

**Injection Flow:**
1. User executes cell
2. Workbooks loads secrets from encrypted storage
3. Decrypts values using system keychain
4. Injects as environment variables into kernel
5. Cell executes with access to secrets
6. Secrets removed from environment after execution

### Auto-Detection System

**Pattern Detection:**
Scans cell code for:
- Long random strings (API keys, tokens)
- Email/password patterns
- Connection strings
- Known secret formats (AWS keys, etc.)

**Detection Algorithm:**
- Regex patterns for common secret formats
- Entropy analysis for random strings
- Whitelist of known secret key names
- Configurable sensitivity

**User Prompts:**
- "This cell appears to contain a secret. Would you like to store it securely?"
- Shows detected value (partially masked)
- Suggests key name based on variable name
- One-click to move to secrets manager

### Output Redaction

**On Save:**
- Scans all cell outputs for secret values
- Replaces exact matches with `[REDACTED]`
- Preserves output structure
- Prevents accidental exposure

**Redaction Scope:**
- stdout/stderr text
- HTML content
- Error messages
- Return values

**Safe to Share:**
- Notebooks can be committed to Git
- Outputs don't leak credentials
- Recipients can run with their own secrets

## Scope

**Project-Wide:**
- All secrets shared across workbooks in a project
- Simplifies management
- Avoids duplication

**Future:**
- Workbook-specific secrets
- Secret inheritance/overrides
- Team/shared secrets

## Migration

**From .env Files:**
- Detect `.env` in project root
- Offer to import into secrets manager
- Encrypt and delete original
- Update code to use `os.environ`

**External Edit Detection:**
- Monitor `.env` or other secret files
- Prompt to import on change
- Help users transition to secure storage

## Use Cases

**Common Secrets:**
- `OPENAI_API_KEY` - OpenAI API key
- `STRIPE_SECRET_KEY` - Stripe API key
- `DATABASE_URL` - Database connection string
- `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` - AWS credentials
- `ANTHROPIC_API_KEY` - Anthropic API key
- `GITHUB_TOKEN` - GitHub personal access token

**Workflow:**
1. User starts new project
2. Adds API key to secrets manager
3. References in notebook via `os.environ`
4. Runs workbook - secrets auto-injected
5. Commits notebook to Git - no secrets exposed
6. Shares with colleague - they add their own secrets
