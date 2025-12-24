# Secrets Injection Fix & Authentication

## What Was Wrong

The error `ERROR: Failed to load secrets: Decryption failed: aead::Error` means:
- The secret exists in the database
- But it was encrypted with a different encryption key than what's currently in your keychain
- This happened because the keyring entry name changed between versions

## Fixes Applied

### 1. **Stable Keyring Entry Names**
- Now using a hash of the full project path instead of the project name
- This prevents issues with special characters or spaces in project names

### 2. **Touch ID Authentication for Editing Secrets**
- When you edit a secret, you now need to authenticate with Touch ID to view the current value
- This prevents shoulder surfing and unauthorized access
- Click "🔐 Authenticate to View Current Value" to see the existing secret

### 3. **Better Error Messages**
- Decryption failures now explain what went wrong and how to fix it

### 4. **Proper Environment Variable Injection**
- Secrets are now correctly passed to the kernel via `env` parameter
- They should appear in `os.environ`

## How to Fix Your Current Project

### Step 1: Delete the old secrets database

```bash
cd "/Users/jmitch/Desktop/Test 4"
rm .workbooks/secrets.db
```

### Step 2: Rebuild and restart the app

```bash
cd /Users/jmitch/Dev/workbooks
npm run tauri dev
```

### Step 3: Re-add your secret

1. Open your project
2. Click "🔐 Secrets" in the sidebar
3. Click "Manage Secrets"
4. Click "+ Add Secret"
5. Add: `DJANGO_ACTIVE` = `yes`
6. Click "Add"

### Step 4: Restart the kernel

In your workbook:
- Click the "Restart" button in the toolbar
- This will reload the secrets into the environment

### Step 5: Test again

Run your test code:

```python
import os

django_active = os.environ.get("DJANGO_ACTIVE")
print(f"DJANGO_ACTIVE = {django_active}")
print(f"Type: {type(django_active)}")
print(f"django_active == 'yes': {django_active == 'yes'}")
```

You should now see:
```
DJANGO_ACTIVE = yes
Type: <class 'str'>
django_active == 'yes': True
```

## New Features

### Touch ID Authentication

When editing a secret:
1. Click the ✏️ edit button
2. You'll see "🔐 Authenticate to View Current Value"
3. Click it to trigger Touch ID
4. After authentication, the current value will load
5. You can then modify it or leave it unchanged

### Debug Output

In the terminal, you'll now see:
```
DEBUG: Using keyring entry: service='workbooks' user='secrets-1a2b3c4d'
DEBUG: Loading secrets from project root: /path/to/project
DEBUG: SecretsManager initialized successfully
DEBUG: Loaded 1 secrets from database
DEBUG: Injecting secret: DJANGO_ACTIVE (length: 3)
DEBUG: Total env_vars after secrets: 2
```

This helps verify that secrets are being loaded and injected correctly.

## Troubleshooting

### If secrets still don't appear in the kernel:

1. **Check the terminal for DEBUG output** - Look for "Injecting secret: ..."
2. **Make sure you restarted the kernel** - Click "Restart" in the workbook toolbar
3. **Verify the secret was added** - Check that the sidebar shows "1 secret"
4. **Check the database exists**: `ls -la "/Users/jmitch/Desktop/Test 4/.workbooks/"`

### If you get "Decryption failed" again:

This means the encryption key in your keychain doesn't match. To reset:

```bash
# On macOS, open Keychain Access app
# Search for "workbooks"
# Delete any entries with service="workbooks"
# Then delete the database and re-add secrets
rm "/Users/jmitch/Desktop/Test 4/.workbooks/secrets.db"
```

## Next: Output Redaction (Coming Soon)

The next feature will be:
- Auto-detect when secrets appear in cell outputs
- Show a warning: "⚠️ Secret value detected in output"
- Provide an "Admin Mode" toggle (requires Touch ID) to see actual values
- Otherwise, redact them as `[REDACTED]`

This will prevent accidental exposure of secrets in notebook outputs when sharing or committing to Git.
