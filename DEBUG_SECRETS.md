# Debug Secrets Not Appearing

Your output shows `DJANGO_ACTIVE = None`, which means the secret isn't being injected into the kernel environment.

## Step 1: Check Terminal Output

Look at the terminal where you ran `npm run tauri dev`. You should see:

```
DEBUG: Loading secrets from project root: /Users/jmitch/Desktop/Test 4
DEBUG: SecretsManager initialized successfully
DEBUG: Loaded 1 secrets from database
DEBUG: Injecting secret: DJANGO_ACTIVE (length: 3)
DEBUG: Total env_vars after secrets: 2
```

**If you see:**
```
ERROR: Failed to load secrets: Decryption failed
```

This means your database has a corrupted secret from before the fix. Continue to Step 2.

## Step 2: Delete Old Database

```bash
# Delete the corrupted database
rm "/Users/jmitch/Desktop/Test 4/.tether/secrets.db"

# Also clean up any old keychain entries (optional but recommended)
# Open "Keychain Access" app
# Search for "tether"
# Delete any entries you find
```

## Step 3: Rebuild and Restart

```bash
cd /Users/jmitch/Dev/tether
npm run tauri dev
```

## Step 4: Re-add Your Secret

1. In the app, open your project
2. Click "🔐 Secrets" in the sidebar
3. Click "Manage Secrets"
4. Click "+ Add Secret"
5. **Key:** `DJANGO_ACTIVE`
6. **Value:** `yes`
7. Click "Add"

You should see:
- Sidebar shows "1 secret"
- Workbook shows 🔐 1 badge

## Step 5: Restart the Kernel

**IMPORTANT:** You must restart the kernel for secrets to be loaded!

1. In your workbook, click the **"Restart"** button in the toolbar
2. Wait for it to say "Engine: Idle"

## Step 6: Test Again

Run this Python code in a cell:

```python
import os

print("=" * 50)
print("SECRETS TEST")
print("=" * 50)

# Test the specific secret
django_active = os.environ.get("DJANGO_ACTIVE")
print(f"\nDJANGO_ACTIVE = {django_active}")
print(f"Type: {type(django_active)}")

if django_active:
    print(f"✓ Secret found!")
    print(f"Match 'yes': {django_active == 'yes'}")
else:
    print(f"✗ Secret NOT found in environment")

# List all environment variables
print(f"\nAll env vars ({len(os.environ)} total):")
for key in sorted(os.environ.keys()):
    if any(x in key.upper() for x in ['DJANGO', 'TETHER', 'SECRET', 'API']):
        value = os.environ[key]
        # Mask long values for security
        if len(value) > 20:
            masked = value[:10] + "..." + value[-10:]
        else:
            masked = value
        print(f"  {key} = {masked}")
```

## Expected Output

```
==================================================
SECRETS TEST
==================================================

DJANGO_ACTIVE = yes
Type: <class 'str'>
✓ Secret found!
Match 'yes': True

All env vars (XX total):
  DJANGO_ACTIVE = yes
  TETHER_PROJECT_FOLDER = /Users/jmitch/Desktop/Test 4
```

## Step 7: Check Terminal for Debug Output

When you restart the kernel, you should see in the terminal:

```
DEBUG: Loading secrets from project root: /Users/jmitch/Desktop/Test 4
DEBUG: Using keyring entry: service='tether' user='secrets-XXXXXXXX'
DEBUG: SecretsManager initialized successfully
DEBUG: Loaded 1 secrets from database
DEBUG: Injecting secret: DJANGO_ACTIVE (length: 3)
DEBUG: Total env_vars after secrets: 2
```

Then in the Python engine output:

```
Injecting env var into kernel: DJANGO_ACTIVE=yes***
Starting engine process with environment variables...
```

## Still Not Working?

If secrets still don't appear after following all steps:

1. **Check the database exists:**
   ```bash
   ls -la "/Users/jmitch/Desktop/Test 4/.tether/"
   ```
   You should see `secrets.db`

2. **Check secrets are in the database:**
   ```bash
   sqlite3 "/Users/jmitch/Desktop/Test 4/.tether/secrets.db" "SELECT key FROM secrets;"
   ```
   Should show: `DJANGO_ACTIVE`

3. **Make sure you restarted the kernel** - secrets are only loaded on kernel start

4. **Check for Python errors in terminal** - Look for any Python tracebacks

5. **Try a different secret name** - Use `TEST_SECRET` instead to rule out name conflicts

## Common Issues

### Issue: "Decryption failed"
**Solution:** Delete `.tether/secrets.db` and re-add secrets

### Issue: Secret appears in sidebar but not in kernel
**Solution:** Restart the kernel (not just the app)

### Issue: No debug output in terminal
**Solution:** Make sure you're looking at the right terminal where `npm run tauri dev` is running

### Issue: Secret was deleted accidentally
**Solution:** Add it again through the UI - they're not auto-recovered
