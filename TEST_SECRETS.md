# Testing Secrets Injection

## Setup

1. **Start the app in dev mode:**
   ```bash
   npm run tauri dev
   ```

2. **Open or create a project**

3. **Add a test secret:**
   - Click on "🔐 Secrets" in the sidebar
   - Click "Manage Secrets"
   - Click "+ Add Secret"
   - Add: `DJANGO_ACTIVE` = `yes`
   - Click "Add"

4. **Create a test workbook** (or open an existing one)

## Test Code

Run this in a code cell:

```python
import os

# Test 1: Check if secret exists
django_active = os.environ.get("DJANGO_ACTIVE")
print(f"DJANGO_ACTIVE = {django_active}")
print(f"Type: {type(django_active)}")

# Test 2: Check the condition
result = django_active == "yes"
print(f"django_active == 'yes': {result}")

# Test 3: List all environment variables (for debugging)
print("\nAll environment variables containing 'DJANGO':")
for key, value in os.environ.items():
    if 'DJANGO' in key.upper():
        print(f"  {key} = {value}")

# Test 4: List all environment variables containing 'WORKBOOKS'
print("\nAll environment variables containing 'WORKBOOKS':")
for key, value in os.environ.items():
    if 'WORKBOOKS' in key.upper():
        print(f"  {key} = {value}")
```

## What to Look For

### In the terminal (where you ran `npm run tauri dev`):

You should see debug output like:
```
DEBUG: Loading secrets from project root: /path/to/project
DEBUG: SecretsManager initialized successfully
DEBUG: Loaded 1 secrets from database
DEBUG: Injecting secret: DJANGO_ACTIVE (length: 3)
DEBUG: Total env_vars after secrets: 2
```

### In the workbook output:

You should see:
```
DJANGO_ACTIVE = yes
Type: <class 'str'>
django_active == "yes": True

All environment variables containing 'DJANGO':
  DJANGO_ACTIVE = yes

All environment variables containing 'WORKBOOKS':
  WORKBOOKS_PROJECT_FOLDER = /path/to/project
```

## Troubleshooting

If the secret is `None`:

1. **Check terminal for DEBUG output** - Did the secrets load?
2. **Check the .workbooks folder exists** - Is there a `.workbooks/secrets.db` file?
3. **Restart the kernel** - Click "Restart" button in the workbook toolbar
4. **Check the secrets.db** - You can inspect it with:
   ```bash
   sqlite3 .workbooks/secrets.db "SELECT id, key, created_at FROM secrets;"
   ```

If you see errors like "Failed to initialize SecretsManager", the database file might not exist yet. Make sure you added a secret through the UI first.

## Expected Behavior

After adding a secret:
1. The sidebar should show "1 secret"
2. The workbook should show 🔐 1 badge
3. The secret should be available in `os.environ`
4. Restarting the kernel should re-inject the secrets
