# Tether

*Sharpen your automations*

Notebooks as automations. Built for speed, security, privacy, open source, and desktop-first. Use our AI Agent to debug and build.

## Feature Documentation

**All feature documentation, implementation status, and todos are tracked in the `features/` directory.**

Each feature area has three files:
- **`docs.md`** - What the feature is, how it works, design decisions
- **`todo.md`** - What needs to be implemented
- **`done.md`** - What has been completed

Anytime a feature todo is done, always move it from todo.md to done.md

### Feature Areas

#### Core UI
- **`features/navigation/`** - Tab-based navigation system, multi-file support
- **`features/sidebar/`** - Sidebar structure and sections (Workbooks, Secrets, Schedule, Files)

#### Workbook System
- **`features/workbooks/`** - Workbook viewer, execution engine, keyboard shortcuts, streaming output
- **`features/files/`** - File management, environment variables (TETHER_PROJECT_FOLDER), file drop handling

#### Data & Security
- **`features/secrets/`** - Secrets management, encryption, keychain integration
- **`features/state/`** - State management system (SQLite, blob storage, tether-core API)

#### Automation
- **`features/schedule/`** - Cron scheduling, run history, automated execution

#### Configuration
- **`features/project-settings/`** - Project settings, package management, export
- **`features/network/`** - Network requirements, offline behavior, status indicators

### Top-Level Feature Files

- **`features/todo.md`** - High-level roadmap and cross-cutting todos
- **`features/changelog.md`** - Chronological list of completed work
- **`features/README.md`** - Feature documentation structure and workflow

**When working on Tether:**
1. Read `features/<area>/docs.md` to understand the design
2. Check `features/<area>/todo.md` for what needs to be done
3. Check `features/<area>/done.md` to see what's already implemented
4. Implement the feature
5. Move completed items from `todo.md` → `done.md`
6. Add entry to `features/changelog.md` with date and description

## Version Management

**CRITICAL: Version numbers must stay synchronized across all files.**

The project version is defined in three locations:
- `package.json` - npm package version
- `src-tauri/Cargo.toml` - Rust crate version
- `src-tauri/tauri.conf.json` - Tauri app version

**When to bump the version:**
- When completing a significant feature or set of features
- Before creating a release or distributing builds
- When the user explicitly asks to bump the version
- When preparing for production deployment

**How to bump the version:**

**For bug fixes and minor changes (patch bump):**
```bash
npm run version
# Auto-increments: 0.1.0 → 0.1.1
```

**For new features (minor bump):**
```bash
npm run version 0.2.0
# Manually specify the new version
```

**For breaking changes (major bump):**
```bash
npm run version 1.0.0
# Manually specify the new version
```

**After bumping:**
1. Review changes with `git diff`
2. Commit with message: "Bump version to X.Y.Z"
3. Tag the commit: `git tag vX.Y.Z`
4. Push: `git push && git push --tags`

**DO NOT manually edit version numbers in individual files** - always use the version script to keep them synchronized.

**The CLI version detection system depends on these versions being in sync** - the app checks installed CLI version and auto-updates if it doesn't match the bundled version.

## Tech Stack Summary

- **Tauri** (Rust + webview) - Native desktop app
- **React 19 + JSX** (NOT TypeScript) - Frontend UI
- **Vite** - Build system with hot reload
- **Monaco Editor** - Code editing
- **React Flow** - Visual pipeline canvas (installed, not yet used)
- **FastAPI + uvicorn** - Python engine server
- **Jupyter Client** - Workbook execution via AsyncKernelManager
- **UV** - Python environment and package management
- **SQLite** - Local state and metadata (planned)

## Current Architecture

The app uses an HTTP-based engine architecture:
1. Tauri starts a FastAPI server (engine_server.py) on port 8765
2. Each workbook gets its own Jupyter engine managed by AsyncKernelManager
3. Engines run in the project's venv, with custom kernel specs installed
4. Frontend communicates via Tauri commands → HTTP → Engine server → Jupyter kernel
5. This allows clean isolation and engine lifecycle management
6. Streaming output is supported through event emission from Rust to frontend

## Project Overview

Tether is a desktop application that makes Jupyter notebooks (called "workbooks" in the UI) durable, resumable, and connectable. Users can run workbooks locally with automatic checkpointing, connect workbooks through shared state, and schedule/orchestrate pipelines visually.

**Brand:** Tether
**CLI:** `tether`
**Extension:** `.tether`
**Domain:** tether.dev

## Core Concepts

### State-Based Connections
Workbooks communicate through a shared state system rather than explicit wiring. Workbooks read/write state variables, and dependencies are inferred automatically.

```python
from tether import state

# Read from state (blocks until available or uses cached)
df = state.get("customers")

# Write to state
state.set("customers_clean", df_clean)
```

### Durability
- Each cell execution creates a checkpoint
- If execution fails or machine sleeps, resume from last checkpoint
- Checkpoints stored locally with optional cloud sync

### Project Structure
```
~/Projects/my-pipeline/
├── .tether/
│   ├── state.db              # SQLite state metadata
│   ├── state/                # Blob storage for large objects
│   │   ├── customers.pkl
│   │   └── model.pkl
│   ├── runs/                 # Execution history
│   └── config.toml           # Project settings
├── .venv/                    # uv-managed virtual environment
├── pyproject.toml            # Dependencies
├── uv.lock                   # Locked versions
├── notebooks/
│   ├── load_data.ipynb
│   ├── transform.ipynb
│   └── train_model.ipynb
└── My Pipeline.tether        # Shortcut file (double-click opens app)
```

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        Tauri App                                │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐             │
│  │   GUI       │  │  Scheduler  │  │  Executor   │             │
│  │  (React)    │  │  (cron)     │  │  (Python)   │             │
│  └─────────────┘  └─────────────┘  └─────────────┘             │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                     uv (bundled)                                │
│  - Creates/manages .venv per project                            │
│  - Installs packages on demand                                  │
│  - Runs workbook execution in isolated env                      │
└─────────────────────────────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                  Local Artifact Store                           │
│  ~/.tether/ or project/.tether/                                 │
│  - state.db (SQLite)                                            │
│  - state/ (blob pickles)                                        │
│  - runs/ (execution logs, checkpoints)                          │
└─────────────────────────────────────────────────────────────────┘
```

## CLI Commands

```bash
tether init <name>                  # Create new project
tether run <workbook.ipynb>         # Run a workbook
tether run --all                    # Run full pipeline
tether status                       # Show pipeline status
tether state list                   # List state variables
tether state inspect <key>          # Inspect a state variable
tether fork <branch-name>           # Fork state (like Neon for SQLite)
tether switch <branch-name>         # Switch state branch
tether schedule <workbook> --cron   # Schedule a workbook
tether logs [workbook]              # View execution logs
tether resume [workbook]            # Resume interrupted run
```

## Key File Locations

### Backend (Rust)
- `src-tauri/src/lib.rs` - Main Tauri app, command registration
- `src-tauri/src/python.rs` - UV integration, environment management
- `src-tauri/src/project.rs` - Project creation and loading
- `src-tauri/src/fs.rs` - File system operations
- `src-tauri/src/engine_http.rs` - HTTP engine server integration
- `src-tauri/engine_server.py` - FastAPI Jupyter engine manager

### Frontend (React)
- `src/App.jsx` - Main app shell, routing, tab management
- `src/components/` - UI components (Welcome, WorkbookViewer, FileViewer, etc.)
- `src/hooks/` - React hooks for project state and Tauri commands

**Implementation status for each component is tracked in the relevant `features/<area>/done.md` and `features/<area>/todo.md` files.**

## State API (Python)

```python
from tether import state

# Basic operations
state.get(key, default=None, wait=False, timeout=None)
state.set(key, value, metadata=None)
state.delete(key)
state.list() -> list[dict]

# Watching for changes
state.watch(key, callback)

# Context for current run
state.run_id
state.workbook_name
```

## Checkpointing Strategy

1. Before each cell executes, save current namespace
2. Filter to picklable objects only
3. Store in `.tether/runs/{run_id}/checkpoints/cell-{n}.pkl`
4. On resume, load latest checkpoint and continue from next cell
5. Chain cell hashes so code changes invalidate downstream checkpoints

## State Forking (Neon-style)

```python
# In StateManager (Rust)
def fork(branch_name):
    # Copy state.db to branches/{branch_name}.db
    # Copy state/ blobs to branches/{branch_name}_blobs/

def switch(branch_name):
    # Swap current state.db with branch version
    # Update blob symlinks/copies
```

Enables: "What if I trained with different params?" Fork, experiment, compare, merge or discard.

## Environment Management

uv is bundled with the Tauri app. Each project gets its own `.venv`.

```rust
// On project open
pub async fn ensure_environment(project_root: &Path) -> Result<()> {
    let venv = project_root.join(".venv");
    if !venv.exists() {
        Command::new_sidecar("uv")
            .args(["venv", ".venv"])
            .current_dir(project_root)
            .output()?;
    }
    
    // Sync deps from pyproject.toml
    Command::new_sidecar("uv")
        .args(["sync"])
        .current_dir(project_root)
        .output()?;
    
    Ok(())
}
```

## Package Auto-Detection

When a workbook fails on import:
1. Parse the cell for import statements
2. Check which packages are missing
3. Prompt user to install
4. Run `uv add <package>` for each

## Shortcut File Format

```json
// My Pipeline.tether
{
  "version": 1,
  "name": "My Pipeline",
  "project_root": "."
}
```

Register `.tether` extension on install. Double-click opens the Tauri app with this project.

## Development Setup

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Node/npm (if not already installed)
# Use your preferred method (nvm, homebrew, etc.)

# Install uv (Python package manager)
curl -LsSf https://astral.sh/uv/install.sh | sh

# Clone and setup
cd tether

# Install frontend dependencies
npm install

# Create Python virtual environment and install dependencies
uv venv
uv sync

# Run in dev mode (starts Tauri app with Vite dev server)
npm run tauri dev

# Build for production
npm run tauri build
```

## Design Principles

1. **Workbooks stay normal notebooks** - Edit in Jupyter, VS Code, wherever. Tether just watches and orchestrates.

2. **State is implicit wiring** - No explicit DAG definition. Write `state.get("x")` and `state.set("y")`, dependencies are inferred.

3. **Local-first** - Everything works offline. Cloud is optional sync/backup.

4. **Durable by default** - Every cell checkpoints. Resume from anywhere.

5. **uv handles Python** - No "install Python first" for users. uv bootstraps everything.

6. **Keep it simple** - Use JSX, not TypeScript, unless absolutely necessary. Minimize complexity.

## UI Design & Style Guide

**All UI components must follow the design patterns defined in `STYLE_GUIDE.md`.**

Key principles:
- **Clean & Minimal** - Professional, understated aesthetic
- **Grayscale + Blue accents** - Consistent color palette across all components
- **No heavy gradients or shadows** - Flat, modern design
- **Tailwind CSS** - Use Tailwind utility classes for all styling
- **Consistent spacing** - Follow Tailwind's spacing scale
- **Semantic colors** - Blue for primary, red for danger, amber for warnings

Before creating or modifying UI components:
1. Read `STYLE_GUIDE.md` to understand approved patterns
2. Reference existing components (Sidebar, WorkbookViewer) for consistency
3. Use the component templates in the style guide
4. Avoid custom CSS unless absolutely necessary (prefer Tailwind utilities)
5. Match the app's professional, minimal aesthetic

## Future Considerations

- Cloud sync for state (S3/R2)
- Team collaboration (shared state branches)
- Claude Code integration (right-click "Edit with Claude")
- GPU scheduling for heavy cells
- Workbook diffing and versioning
- Package auto-detection and installation prompts
- Variable inspector / debugger integration