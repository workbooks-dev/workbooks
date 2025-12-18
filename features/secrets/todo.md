# Secrets - To Do

## Backend (Rust)

- [ ] Encryption system
  - [ ] System keychain integration (macOS Keychain, Windows Credential Manager, Linux Secret Service)
  - [ ] Touch ID authentication on macOS
  - [ ] SQLite database for encrypted secrets storage
  - [ ] Encryption/decryption functions
  - [ ] CRUD operations (create, read, update, delete secrets)

- [ ] Runtime injection
  - [ ] Inject secrets as environment variables into kernel
  - [ ] Scope secrets to workbook execution
  - [ ] Remove secrets from environment after execution
  - [ ] Secure memory handling

- [ ] Tauri commands
  - [ ] `add_secret(key, value)` - Add new secret
  - [ ] `get_secret(key)` - Retrieve decrypted secret
  - [ ] `list_secrets()` - List all secret keys (not values)
  - [ ] `update_secret(key, value)` - Update existing secret
  - [ ] `delete_secret(key)` - Delete secret
  - [ ] `import_from_env(path)` - Import .env file

## Frontend (React)

- [ ] Secrets management tab
  - [ ] Table view of all secrets
  - [ ] Add secret dialog
  - [ ] Edit secret dialog
  - [ ] Delete confirmation
  - [ ] Search/filter functionality

- [ ] Sidebar integration
  - [ ] Show secret count in Secrets section
  - [ ] Click header to open secrets tab
  - [ ] Quick "Add Secret" action

- [ ] WorkbookViewer integration
  - [ ] Lock icon when secrets are active
  - [ ] List of available secrets
  - [ ] Secret detection dialog
  - [ ] Cell rewriting functionality

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

## Testing & Documentation

- [ ] Security audit of encryption implementation
- [ ] Test keychain integration on all platforms
- [ ] Test secret injection into kernels
- [ ] Test output redaction accuracy
- [ ] User documentation for secrets workflow
