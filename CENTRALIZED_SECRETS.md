# Centralized Secrets Storage

## New Architecture

Secrets are now stored **centrally** in your home directory, **NOT** in the project folder.

### Storage Location

```
~/.tether/
├── secrets/
│   └── {project-hash}/
│       ├── secrets.db          # Encrypted secrets for this project
│       └── project_path.txt    # Reference to the project location
└── venvs/                      # (existing) Python virtual environments
```

**Your project folder stays clean:**
```
my-project/
├── .tether/
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

1. **Keeps projects clean** - `.tether/` only contains shareable data
2. **Machine-specific** - Each developer has their own secrets
3. **Safe for version control** - No risk of committing secrets
4. **Organized** - All your secrets in one place (`~/.tether/secrets/`)
5. **Easy cleanup** - Delete `~/.tether/secrets/{hash}` to remove project secrets

### 🔐 Security

- Secrets stored in `~/.tether/secrets/` (encrypted with AES-256-GCM)
- Encryption keys in system keychain (Touch ID protected)
- Project folder can be safely committed to git

## Migration from Old Location

If you had secrets in the old location (`project/.tether/secrets.db`), they won't work anymore.

**To migrate:**

1. **Note your existing secrets** (write them down or screenshot)
2. **Delete old database:**
   ```bash
   rm "/path/to/project/.tether/secrets.db"
   ```
3. **Restart the app** and **re-add your secrets** via the UI
4. Secrets will now be stored at: `~/.tether/secrets/{project-hash}/secrets.db`

## Finding Your Secrets

To find where a project's secrets are stored:

```bash
# The app will print this when loading secrets
# Look for: "DEBUG: Secrets stored at: ..."

# Or check manually:
ls ~/.tether/secrets/
# Each folder has a project_path.txt showing which project it's for
cat ~/.tether/secrets/*/project_path.txt
```

## Cleanup

To delete secrets for a project:

```bash
# Find the hash for your project
grep -l "/path/to/your/project" ~/.tether/secrets/*/project_path.txt

# Delete that folder
rm -rf ~/.tether/secrets/{hash-from-above}/
```

Or just delete the entire secrets directory:
```bash
rm -rf ~/.tether/secrets/
```

Then re-add secrets via the UI.

## Future: `.env.tether` Support

**Coming soon:** Optional encrypted `.env.tether` file for team sharing:
- Encrypted with a team-shared key
- Can be committed to git
- Team members decrypt with shared passphrase
- Falls back to machine-specific secrets in `~/.tether/secrets/`

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
   DEBUG: Secrets stored at: /Users/you/.tether/secrets/1617280d59bcfd33/secrets.db
   DEBUG: Using keyring entry: service='tether' user='secrets-1617280d59bcfd33'
   ```

4. **Verify the file was created:**
   ```bash
   ls ~/.tether/secrets/
   ```

5. **Restart kernel and test:**
   ```python
   import os
   print(os.environ.get("DJANGO_ACTIVE"))  # Should print: yes
   ```

## Project Folder is Now Git-Safe

You can safely commit `.tether/` to git:

```bash
# .gitignore (optional - you may want to commit .tether/ for state sharing)
# .tether/runs/           # Execution logs (optional)
```

Secrets are never in the project folder, so they can't be accidentally committed!
