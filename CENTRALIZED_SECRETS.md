# Centralized Secrets Storage

## New Architecture

Secrets are now stored **centrally** in your home directory, **NOT** in the project folder.

### Storage Location

```
~/.workbooks/
├── secrets/
│   └── {project-hash}/
│       ├── secrets.db          # Encrypted secrets for this project
│       └── project_path.txt    # Reference to the project location
└── venvs/                      # (existing) Python virtual environments
```

**Your project folder stays clean:**
```
my-project/
├── .workbooks/
│   ├── state.db               # ✅ Shareable - state metadata
│   ├── state/                 # ✅ Shareable - state blobs (can be synced)
│   ├── runs/                  # ✅ Shareable - execution history
│   └── config.toml            # ✅ Shareable - project settings
├── notebooks/
└── pyproject.toml
```

**Secrets are machine-specific** and never committed to git!

## Why This Change?

### ✅ Benefits

1. **Keeps projects clean** - `.workbooks/` only contains shareable data
2. **Machine-specific** - Each developer has their own secrets
3. **Safe for version control** - No risk of committing secrets
4. **Organized** - All your secrets in one place (`~/.workbooks/secrets/`)
5. **Easy cleanup** - Delete `~/.workbooks/secrets/{hash}` to remove project secrets

### 🔐 Security

- Secrets stored in `~/.workbooks/secrets/` (encrypted with AES-256-GCM)
- Encryption keys in system keychain (Touch ID protected)
- Project folder can be safely committed to git

## Migration from Old Location

If you had secrets in the old location (`project/.workbooks/secrets.db`), they won't work anymore.

**To migrate:**

1. **Note your existing secrets** (write them down or screenshot)
2. **Delete old database:**
   ```bash
   rm "/path/to/project/.workbooks/secrets.db"
   ```
3. **Restart the app** and **re-add your secrets** via the UI
4. Secrets will now be stored at: `~/.workbooks/secrets/{project-hash}/secrets.db`

## Finding Your Secrets

To find where a project's secrets are stored:

```bash
# The app will print this when loading secrets
# Look for: "DEBUG: Secrets stored at: ..."

# Or check manually:
ls ~/.workbooks/secrets/
# Each folder has a project_path.txt showing which project it's for
cat ~/.workbooks/secrets/*/project_path.txt
```

## Cleanup

To delete secrets for a project:

```bash
# Find the hash for your project
grep -l "/path/to/your/project" ~/.workbooks/secrets/*/project_path.txt

# Delete that folder
rm -rf ~/.workbooks/secrets/{hash-from-above}/
```

Or just delete the entire secrets directory:
```bash
rm -rf ~/.workbooks/secrets/
```

Then re-add secrets via the UI.

## Future: `.env.workbooks` Support

**Coming soon:** Optional encrypted `.env.workbooks` file for team sharing:
- Encrypted with a team-shared key
- Can be committed to git
- Team members decrypt with shared passphrase
- Falls back to machine-specific secrets in `~/.workbooks/secrets/`

## Testing the New Location

After rebuilding:

1. **Start the app:**
   ```bash
   npm run tauri dev
   ```

2. **Add a secret:**
   - Sidebar → 🔐 Secrets → Manage Secrets → + Add Secret
   - Add: `DJANGO_ACTIVE` = `yes`

3. **Check the terminal** - you should see:
   ```
   DEBUG: Secrets stored at: /Users/you/.workbooks/secrets/1617280d59bcfd33/secrets.db
   DEBUG: Using keyring entry: service='workbooks' user='secrets-1617280d59bcfd33'
   ```

4. **Verify the file was created:**
   ```bash
   ls ~/.workbooks/secrets/
   ```

5. **Restart kernel and test:**
   ```python
   import os
   print(os.environ.get("DJANGO_ACTIVE"))  # Should print: yes
   ```

## Project Folder is Now Git-Safe

You can safely commit `.workbooks/` to git:

```bash
# .gitignore (optional - you may want to commit .workbooks/ for state sharing)
# .workbooks/runs/           # Execution logs (optional)
```

Secrets are never in the project folder, so they can't be accidentally committed!
