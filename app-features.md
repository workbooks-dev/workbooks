# Tether App Features

## Overview

Each Tether window is a **project** - a workspace for running workbooks with secure secrets and scheduling. The interface is designed to be practical and task-oriented, helping users quickly:

1. **Create and run workbooks** (Jupyter notebooks with superpowers)
2. **Securely manage API keys and credentials** (encrypted, never exposed)
3. **Schedule automated runs** (set it and forget it)
4. **Access project data files** (data, scripts, anything you need)

## Network Requirements & Status

Tether is primarily a local-first app, but **requires internet connection for initial setup** and package management.

### When Internet is Required

**First-Time Setup (per machine):**
- Installing `uv` (Python package manager) - downloads from astral.sh
- User should see: "Installing uv..." → "uv installed successfully"

**Project Creation/Opening (per project):**
- Python installation via uv (if Python not already available)
- User should see: "Installing Python 3.12..." → "Python 3.12 installed"
- Installing core dependencies (jupyter, nbformat, ipykernel, etc.)
- User should see: "Installing Python packages..." → "Packages installed successfully"

**During Development:**
- Adding new packages via Project Settings
- User should see: "Installing [package name]..." → "[package name] installed"
- Package updates or dependency resolution
- User should see: "Updating dependencies..." → "Dependencies updated"

### Offline Behavior (TO BE IMPLEMENTED)

When the app is offline and user attempts network-dependent operations:

1. **Creating a new project** → Show error: "Cannot create project - Internet connection required to install Python and dependencies"
2. **Opening an existing project** (if Python/packages not yet installed) → Show error: "Cannot open project - Internet connection required to complete setup"
3. **Adding packages** → Show error: "Cannot install packages - Internet connection required"
4. **First-time uv installation** → Show error: "Cannot install uv - Internet connection required"

**Status Indicators Needed:**
- Network status indicator in UI (online/offline)
- Progress indicators during downloads/installations
- Clear error messages when offline operations are attempted
- Ability to retry failed network operations when connection is restored

**Once Set Up:**
- Projects that are fully initialized can run completely offline
- No internet required for executing workbooks with existing packages
- Secrets, scheduling, and file access work offline


## File Drop Behavior

When a user drops files into Tether:

- **Notebooks (`.ipynb`)** → Saved to `/notebooks` folder, appear in Workbooks sidebar
- **Everything else** → Saved to project root, appear in Files sidebar 

## Sidebar Navigation

The sidebar provides quick access to everything in your project, organized by what you're trying to accomplish.

### 📓 Workbooks

**What it shows:**
- List of all `.ipynb` files in your project
- Ordered by **most recently used** (your active work floats to the top)
- Click to open in the main editor

**Full Workbooks View:**
When you click the "Workbooks" header, you get a full table view with:
- **Name** - Workbook filename
- **Last Run** - When it last executed
- **Status** - Success/Failed/Never run
- **Scheduled** - Is it automated? (Yes/No or frequency)
- **Actions** - Quick buttons to Run, Schedule, etc.

This view lets you filter and sort when you have many workbooks.

**Workflow:**
- First-time users: Start with templates like "Get Stripe Orders" or create from scratch
- Advanced users: Just create new workbooks and start coding
- Migrating users: Import existing `.ipynb` files from Google Colab or Jupyter

### 🔐 Secrets

**What it shows:**
- Table of encrypted key/value pairs
- Examples: `OPENAI_API_KEY`, `STRIPE_SECRET_KEY`, `DATABASE_URL`
- Includes API keys, passwords, connection strings, anything sensitive

**How it works:**
- Click "+ Add Secret" to add a new key/value pair
- All values are **encrypted** using your system keychain (Touch ID on macOS)
- Secrets are automatically injected when workbooks run
- **Auto-detection**: If you hardcode a secret in your code, Tether detects it and offers to securely store it
- See `encryption.md` for full security details

**Scope:**
- All secrets are **project-wide** for now (shared across all workbooks in the project)
- If you need different secrets, create a new project

**Use cases:**
- API keys for services (OpenAI, Stripe, AWS, etc.)
- Database connection strings (PostgreSQL, MySQL, MongoDB)
- Authentication tokens
- Any credential you don't want to hardcode

### ⏰ Schedule

**What it shows:**

**Tab 1: Scheduled Workbooks**
- List of workbooks with active schedules
- Shows: `"Get Stripe Orders - Daily at 9am"`
- Next run time displayed
- Toggle to enable/disable schedules

**Tab 2: Recent Runs**
- Last **30 runs** across all workbooks
- Shows: Workbook name, timestamp, duration, status (success/failed)
- Click to view the run report (saved notebook output)
- After 30 runs, oldest are automatically deleted

**Storage:**
- Run reports stored in `.tether/runs/` (implementation TBD)
- Each run saves the notebook output so you can review what happened
- Useful for debugging failures or auditing automated runs

**Workflow:**
- Click "+ Add Schedule" to automate a workbook
- Choose frequency (daily, hourly, weekly, cron expression)
- Tether runs it in the background (app must be running)

### 📁 Files

**What it shows:**
- All files in your project (except `.ipynb` files, which appear in Workbooks)
- Data files (CSV, Excel, SQLite, JSON, etc.)
- Python scripts (`.py` files)
- Markdown docs
- Any other project files
- Full file tree structure (respects your organization)

**How workbooks access files:**
- Environment variable `TETHER_PROJECT_FOLDER` available in all workbooks
- Points to the project root (absolute path)
- Example: `pd.read_csv(os.path.join(os.environ["TETHER_PROJECT_FOLDER"], "sales_data.csv"))`
- Or for organized projects: `os.path.join(os.environ["TETHER_PROJECT_FOLDER"], "data/sales.csv")`

**Workflow:**
- Drag and drop files into Tether (saved to project root)
- Organize in subdirectories however you like
- Files sidebar reflects your actual folder structure
- Opening a notebook from Files also shows it in Workbooks sidebar

## Bottom of Sidebar: Project Settings

**Gear icon (⚙️) → Project Settings**

Opens a modal/panel with:

1. **Project Name** (editable)
   - Updates the `.tether` shortcut file

2. **Python Packages**
   - Shows currently installed packages
   - Add/remove packages (updates `pyproject.toml` and syncs with `uv`)
   - Shows default packages for new projects (see `project-defaults.md`)

3. **Export Project** (optional)
   - Zip the entire project folder for sharing
   - Includes workbooks, files, encrypted secrets (`.env.tether`)
   - Recipient will need to set up their own secrets when they open it

**Note:** No "Delete Project" button - it's just a folder on your computer. Delete it like any other folder if needed.

## Implementation Status

See `CLAUDE.md` for detailed implementation status of each component.

**Key Points:**
- Sidebar redesigned with multi-section layout (Workbooks, Secrets, Schedule, Files, Settings)
- Workbooks section functional with recent-use ordering and table view
- TETHER_PROJECT_FOLDER environment variable now injected into all workbook kernels
- Secrets system is fully designed (see `encryption.md`) but not yet implemented
- Schedule system is not yet implemented


## Checklist

### Sidebar UI (MVP Skeleton)
- [x] Created new multi-section Sidebar component
- [x] Workbooks section with recent-use ordering
- [x] Workbooks table view modal (click header to view)
- [x] Secrets section placeholder with lock icon
- [x] Schedule section placeholder with two-tab structure
- [x] Files section (filters out .ipynb files, shows in Workbooks instead)
- [x] Project Settings gear icon at bottom
- [x] Integrated Sidebar into App.jsx

### Workbooks Section
- [x] List of .ipynb files from /notebooks folder
- [x] Recent-use ordering (tracks last 20 opened workbooks)
- [x] Click to open workbook
- [x] New Workbook button
- [x] Table view modal with columns: Name, Last Run, Status, Scheduled, Actions
- [ ] Persist last run times (currently shows "Never")
- [ ] Persist run status (currently shows "Not Run")
- [ ] Persist schedule info (currently shows "No")
- [ ] Functional Run button in table view
- [ ] Functional Schedule button in table view

### Secrets Management
- [ ] Backend encryption system (Rust)
- [ ] System keychain integration (Touch ID on macOS)
- [ ] Secrets UI component
- [ ] Add/edit/delete secrets
- [ ] Auto-detection of hardcoded secrets in cells
- [ ] Cell rewriting to use os.environ
- [ ] Output redaction on save
- [ ] Migration from .env files
- [ ] External edit detection

### Schedule System
- [ ] Scheduler backend (Rust)
- [ ] Cron-based scheduling
- [ ] Schedule UI (add/edit/delete schedules)
- [ ] Recent Runs tab with last 30 runs
- [ ] Run reports storage in .tether/runs/
- [ ] Background execution while app is running

### File Management
- [x] Files section shows non-.ipynb files
- [x] TETHER_PROJECT_FOLDER environment variable injection
- [ ] File drop behavior (.ipynb → /notebooks, others → root)
- [ ] Drag-and-drop file upload

### Project Settings
- [ ] Project Settings modal
- [ ] Edit project name
- [ ] Python package management UI
- [ ] Export project as ZIP
- [ ] Default packages for new projects (see project-defaults.md)

### WorkbookViewer Enhancements
- [x] Cell execution with streaming output
- [x] Rich output rendering (images, HTML, tables)
- [x] Kernel lifecycle management
- [x] Keyboard shortcuts (DD, A/B, M/Y, arrows)
- [ ] Lock icon when secrets are active
- [ ] Secret detection dialog before cell execution
- [ ] Output redaction integration

### State Management (Future)
- [ ] SQLite state.db
- [ ] Blob storage for large objects
- [ ] Python tether-core package
- [ ] state.get() / state.set() API
- [ ] Automatic dependency tracking
- [ ] State forking (Neon-style branches)

### Other Features
- [ ] React Flow canvas for visual pipeline connections
- [ ] Run logs and execution history
- [ ] Checkpointing and resume functionality
- [ ] .tether file association (double-click to open)
- [ ] Package auto-detection on import errors