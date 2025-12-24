# Network Requirements & Offline Behavior

## Overview

Workbooks is primarily a **local-first app** - all data and execution happens on your machine. However, it **requires internet connection for initial setup** and package management.

## Design Philosophy

**Local-First:**
- Data never leaves your machine
- No cloud services required for core functionality
- Full control over your data
- Works offline once set up

**Internet for Setup:**
- One-time downloads (Python, packages)
- Package manager infrastructure (uv, PyPI)
- Transparent to user with clear status messages

## When Internet is Required

### First-Time Setup (Per Machine)

**Installing uv (Python package manager):**
- Downloads from `https://astral.sh/uv/install.sh` (Unix)
- Downloads from `https://astral.sh/uv/install.ps1` (Windows)
- Only needed once per machine
- User sees: "Installing uv..." → "uv installed successfully"

**Status Messages:**
```
Installing uv...
[Progress indicator]
uv installed successfully
```

### Project Creation/Opening (Per Project)

**Python Installation:**
- uv downloads Python if not already available
- User sees: "Installing Python 3.12..." → "Python 3.12 installed"

**Core Dependencies:**
- jupyter, nbformat, ipykernel, cloudpickle, etc.
- User sees: "Installing Python packages..." → "Packages installed successfully"

**Status Messages:**
```
Installing Python 3.12...
[Progress indicator]
Python 3.12 installed

Installing Python packages...
[Progress indicator]
Packages installed successfully
```

### During Development

**Adding Packages:**
- Via Project Settings → Add Package
- Downloads from PyPI
- User sees: "Installing [package name]..." → "[package name] installed"

**Package Updates:**
- When syncing dependencies
- User sees: "Updating dependencies..." → "Dependencies updated"

**Status Messages:**
```
Installing pandas...
[Progress indicator]
pandas installed

Updating dependencies...
[Progress indicator]
Dependencies updated
```

## Offline Behavior

### What Works Offline

**Once a project is fully set up:**
- ✅ Execute workbooks with existing packages
- ✅ Create new workbooks
- ✅ Edit files
- ✅ Manage secrets (local encryption)
- ✅ Run scheduled workbooks
- ✅ Access file system
- ✅ All core functionality

**No internet required for:**
- Running code
- Saving files
- Local state management
- File operations
- UI interactions

### What Doesn't Work Offline

**Without internet:**
- ❌ Creating new projects (needs Python/packages)
- ❌ Opening projects not yet initialized (needs setup)
- ❌ Installing new packages
- ❌ First-time uv installation
- ❌ Updating dependencies

**Error Messages:**

When attempting to create project offline:
```
Cannot create project - Internet connection required

Workbooks needs to download Python and core packages
to initialize your project. Please connect to the
internet and try again.

[Retry] [Cancel]
```

When attempting to install packages offline:
```
Cannot install packages - Internet connection required

Package installation requires access to PyPI.
Please connect to the internet and try again.

[Retry] [Cancel]
```

When opening uninitialized project offline:
```
Cannot complete project setup - Internet connection required

This project needs Python and dependencies installed.
Please connect to the internet to finish setup.

[Retry] [Cancel]
```

## Status Indicators (To Implement)

### Network Status Display

**Location:** Top-right corner of app (or status bar)

**States:**
- 🟢 Online - Full functionality available
- 🔴 Offline - Limited to existing projects
- 🟡 Checking... - Verifying connection

**User Benefit:**
- Know why operations are failing
- Clear when to expect internet-dependent features

### Progress Indicators

**During Downloads:**
- Show what's being downloaded
- Progress bar (if available from uv/PyPI)
- Estimated time (if available)
- Cancel button for long operations

**Example:**
```
Installing numpy...
████████████░░░░░░░░ 65%
12 MB / 18 MB
[Cancel]
```

### Operation Status

**Real-time feedback:**
- "Downloading Python 3.12..."
- "Extracting packages..."
- "Installing jupyter..."
- "Syncing dependencies..."
- "Installation complete!"

**Error Handling:**
- Clear error messages
- Suggest solutions (check internet, retry, etc.)
- Retry button
- Link to troubleshooting docs (future)

## Retry Mechanism

**Automatic Retries:**
- Network errors retry 3 times
- Exponential backoff
- User sees retry count

**Manual Retry:**
- "Retry" button in error dialogs
- Re-attempt operation
- Resume from last checkpoint if possible

**Example:**
```
Failed to download package (attempt 1/3)
Retrying in 2 seconds...
```

## Network Checks

**Before Operations:**
- Check connectivity before starting downloads
- Early failure if offline
- Avoid partial operations

**Connection Detection:**
- Ping PyPI or astral.sh
- Quick timeout (1-2 seconds)
- Cache result briefly

**Fallback:**
- If check fails but operation might succeed, try anyway
- Some networks block pings but allow downloads

## User Education

**First Run:**
- Show tooltip or message explaining internet requirement
- "Workbooks needs to download Python and packages for first-time setup"
- Set expectations

**Documentation:**
- Clear explanation of what requires internet
- FAQ about offline usage
- Troubleshooting guide

## Future Enhancements

**Offline Package Cache:**
- Cache downloaded packages locally
- Reuse across projects
- Reduce re-downloads
- uv already does some of this

**Offline Mode Indicator:**
- Prominent offline mode UI
- List of unavailable features
- Guide to offline capabilities

**Pre-Download:**
- "Download for offline use" option
- Pre-fetch common packages
- Prepare for offline work

**Connection Resilience:**
- Resume interrupted downloads
- Partial package installation
- Better error recovery

## Implementation Notes

**Current State:**
- uv installation downloads at runtime (via `python.rs`)
- No network status indicator
- No offline error messages
- No retry mechanism

**Priority:**
1. Network status indicator
2. Clear offline error messages
3. Retry functionality
4. Progress indicators
5. User education/docs
