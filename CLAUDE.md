# Tether

Durable workbook orchestration for local-first data pipelines.

## Current Implementation Status

**What's Built:**

### Core Infrastructure
- Tauri app scaffolding (Rust backend, React 19 frontend)
- Vite build system with hot reload
- Application state management (current project tracking)

### Backend (Rust)
- **Python/UV Integration (`src-tauri/src/python.rs`)**:
  - Automatic uv installation if not present
  - Virtual environment creation and management
  - Package installation (jupyter, nbformat, cloudpickle, ipykernel)
  - Python code execution in isolated venv
  - Dependency syncing with `uv sync`

- **Project Management (`src-tauri/src/project.rs`)**:
  - `create_project()` - Initialize new Tether project with uv
  - `open_folder()` - Open any folder with pyproject.toml
  - `load_project()` - Load existing project or .tether shortcut
  - Creates directory structure (.tether/, notebooks/, pyproject.toml)
  - Generates .tether shortcut file

- **File System Operations (`src-tauri/src/fs.rs`)**:
  - `list_files()` - Directory listing with file metadata
  - `create_workbook()` - Create new .ipynb files with proper structure
  - `read_workbook()` - Load workbook JSON
  - `save_workbook()` - Save workbook with validation
  - `read_file()` - Read any file (text)
  - `save_file()` - Save any file
  - `rename_file()` - Rename files
  - `delete_file()` - Delete files
  - `duplicate_workbook()` - Duplicate a workbook with new name

- **Jupyter Engine Integration**:
  - **HTTP Engine Server (`src-tauri/src/engine_http.rs` + `engine_server.py`)**:
    - FastAPI server for engine lifecycle management
    - Per-workbook engine isolation
    - Health check endpoint
    - `start_engine()` - Start engine in project venv
    - `execute_cell()` - Execute code and collect outputs
    - `execute_cell_stream()` - Execute code with streaming output
    - `stop_engine()` - Clean shutdown
    - `interrupt_engine()` - Interrupt running execution
    - `restart_engine()` - Restart engine and clear state
  - **Direct ZMQ Integration (`src-tauri/src/kernel.rs`)**:
    - Low-level Jupyter protocol implementation (not currently used)
    - ZMQ socket management
    - Message serialization/deserialization
    - Kernel spec discovery

- **Tauri Commands**:
  - UV: `check_uv_installed`, `install_uv`, `ensure_uv`
  - Projects: `create_project`, `open_folder`, `load_project`, `get_current_project`, `set_project_root`, `get_project_root`
  - Python: `init_python_env`, `ensure_python_venv`, `install_python_package`, `install_python_packages`, `run_python_code`
  - Files: `list_files`, `create_workbook`, `read_workbook`, `save_workbook`, `read_file`, `save_file`, `rename_file`, `delete_file`, `duplicate_workbook`
  - Engines: `ensure_engine_server`, `start_engine`, `execute_cell`, `execute_cell_stream`, `stop_engine`, `interrupt_engine`, `restart_engine`

### Frontend (React + JSX)
- **App Shell (`src/App.jsx`)**:
  - Multi-view routing (welcome, create, project)
  - Project state management
  - Tab system for multiple open files
  - Workbook and file viewer integration
  - Autosave toggle

- **Components**:
  - `Welcome.jsx` - Landing screen with "Open Folder" and "Create New Project"
  - `CreateProject.jsx` - New project wizard
  - `FileExplorer.jsx` - Collapsible tree view with file icons, workbook creation, context menu support
  - `TabBar.jsx` - Tab management for open files:
    - Multiple file type support (workbooks, Python, Markdown, JSON, etc.)
    - Tab close functionality
    - Autosave toggle control
    - Active tab highlighting
  - `WorkbookViewer.jsx` - Full-featured workbook editor:
    - Monaco editor for code cells
    - Markdown cell editing and rendering
    - Cell execution with Shift+Enter (and streaming output)
    - Output rendering (stdout, stderr, execute_result, errors, images)
    - Cell manipulation (add, delete, move up/down, change type, clear output)
    - Jupyter-style keyboard shortcuts (a/b for add, m/y for type change, arrows for navigation)
    - Engine lifecycle management (start, stop, interrupt, restart)
    - Auto-save support with toggle
    - ANSI color code stripping
  - `FileViewer.jsx` - General file editor (BUILT):
    - Monaco editor for code files
    - Markdown preview for .md files
    - Syntax highlighting for multiple languages
    - Save functionality (Cmd/Ctrl+S)
    - Unsaved changes detection
  - `ContextMenu.jsx` - Right-click context menu (BUILT):
    - File operations (rename, delete, duplicate)
    - Positioned near cursor
    - Click-outside and Escape to close
  - `InputDialog.jsx` - Modal input dialog (BUILT):
    - Used for rename/duplicate operations
    - Keyboard shortcuts (Enter to confirm, Escape to cancel)
    - Auto-focus and text selection
  - `Canvas.jsx` - Placeholder for React Flow (not implemented)
  - `StatePanel.jsx` - Placeholder for state inspector (not implemented)
  - `WorkbookList.jsx` - Placeholder (not implemented)
  - `RunLog.jsx` - Placeholder (not implemented)

- **Hooks**:
  - `useProject.js` - Project state hooks (exists but implementation TBD)
  - `useTether.js` - Tauri command wrappers (exists but implementation TBD)

### Python Runtime
- **Engine Server (`src-tauri/engine_server.py`)**:
  - FastAPI + uvicorn HTTP server
  - AsyncKernelManager for each workbook
  - Automatic kernel spec installation per project
  - Connection to project venv Python
  - IOPub message collection
  - Streaming output support
  - Interrupt and restart capabilities
  - Graceful shutdown with cleanup

- **Dependencies (`pyproject.toml`)**:
  - fastapi, uvicorn - HTTP server
  - jupyter-client - Engine/kernel management

### Package Dependencies
- **Frontend (`package.json`)**:
  - React 19
  - Monaco Editor (@monaco-editor/react)
  - React Flow (@xyflow/react) - installed but not used
  - Tauri plugins (dialog, opener)

**What's NOT Built (Yet):**
- State management system (SQLite state.db, blob storage)
- Checkpointing and durability (cell-by-cell checkpoints)
- Notebook dependency tracking and auto-discovery
- React Flow canvas UI for visual pipeline connections
- Run logs and execution history viewer
- Python tether-core package (the `from tether import state` API)
- Scheduler/cron functionality
- State forking (Neon-style branches)
- .tether file association and double-click to open

**Architecture Notes:**
The app currently uses an HTTP-based kernel architecture:
1. Tauri starts a FastAPI server (kernel_server.py) on port 8765
2. Each notebook gets its own Jupyter kernel managed by AsyncKernelManager
3. Kernels run in the project's venv, with custom kernel specs installed
4. Frontend communicates via Tauri commands → HTTP → Kernel server → Jupyter kernel
5. This allows clean isolation and kernel lifecycle management

**Next Steps:**
Priority features to implement:
1. State management system (tether-core Python package + SQLite backend)
2. React Flow canvas for visualizing notebook connections
3. Run logs and execution history
4. Checkpointing and resume functionality
5. Scheduler for automated runs

## Project Overview

Tether is a desktop application that makes Jupyter notebooks durable, resumable, and connectable. Users can run notebooks locally with automatic checkpointing, connect notebooks through shared state, and schedule/orchestrate pipelines visually.

**Brand:** Tether
**CLI:** `tether`
**Extension:** `.tether`
**Domain:** tether.dev

## Core Concepts

### State-Based Connections
Notebooks communicate through a shared state system rather than explicit wiring. Notebooks read/write state variables, and dependencies are inferred automatically.

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

## Tech Stack

### Desktop App
- **Tauri** (Rust + webview) - lightweight native app
- **React** + **JSX** (NOT TypeScript unless absolutely necessary) - frontend
- **React Flow / Xyflow** - drag-and-drop canvas for connecting notebooks
- **Monaco** - code preview
- **SQLite** - local state and run history

### Python Runtime
- **uv** - bundled for environment management, package installation
- **cloudpickle** - serialize notebook namespace between cells
- **nbformat** - parse and execute notebooks

### State Storage
- **SQLite** - metadata, small values, run tracking
- **Filesystem blobs** - large objects (DataFrames, models) as pickles
- **Optional S3** - cloud backup/sync

## Architecture

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
│  - Runs notebook execution in isolated env                      │
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
tether run <notebook.ipynb>         # Run a notebook
tether run --all                    # Run full pipeline
tether status                       # Show pipeline status
tether state list                   # List state variables
tether state inspect <key>          # Inspect a state variable
tether fork <branch-name>           # Fork state (like Neon for SQLite)
tether switch <branch-name>         # Switch state branch
tether schedule <notebook> --cron   # Schedule a notebook
tether logs [notebook]              # View execution logs
tether resume [notebook]            # Resume interrupted run
```

## Key Files Status

### Rust (src-tauri/) - Built
- ✅ `src/lib.rs` - Tauri app entry, command handlers (BUILT)
- ✅ `src/python.rs` - uv integration, environment management (BUILT)
- ✅ `src/project.rs` - Project initialization and loading (BUILT)
- ✅ `src/fs.rs` - File system operations (BUILT)
- ✅ `src/kernel.rs` - Direct ZMQ kernel integration (BUILT but not used)
- ✅ `src/kernel_http.rs` - HTTP kernel server integration (BUILT and in use)
- ❌ `src/state.rs` - State management, forking (NOT BUILT)
- ❌ `src/executor.rs` - Notebook execution orchestration (NOT BUILT)
- ❌ `src/scheduler.rs` - Cron-based scheduling (NOT BUILT)

### React/JSX (src/) - Partially Built
- ✅ `App.jsx` - Main app layout with routing (BUILT)
- ✅ `components/Welcome.jsx` - Landing screen (BUILT)
- ✅ `components/CreateProject.jsx` - New project wizard (BUILT)
- ✅ `components/FileExplorer.jsx` - File tree browser (BUILT)
- ✅ `components/NotebookViewer.jsx` - Full notebook editor (BUILT)
- ⚠️ `components/Canvas.jsx` - React Flow visualization (PLACEHOLDER)
- ⚠️ `components/StatePanel.jsx` - State inspector (PLACEHOLDER)
- ⚠️ `components/NotebookList.jsx` - Notebook list (PLACEHOLDER)
- ⚠️ `components/RunLog.jsx` - Execution logs (PLACEHOLDER)
- ⚠️ `hooks/useProject.js` - Project state hooks (EXISTS, needs implementation)
- ⚠️ `hooks/useTether.js` - Tauri command wrappers (EXISTS, needs implementation)

### Python (tether-core/) - Not Built
- ✅ `kernel_server.py` - FastAPI kernel manager (BUILT, in src-tauri/)
- ❌ `tether/__init__.py` - Public API (state) (NOT BUILT)
- ❌ `tether/state.py` - State get/set/list/delete (NOT BUILT)
- ❌ `tether/executor.py` - Cell-by-cell execution with checkpointing (NOT BUILT)
- ❌ `tether/checkpoint.py` - Namespace serialization (NOT BUILT)
- ❌ `tether/cli.py` - CLI entry point (NOT BUILT)

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
state.notebook_name
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

When a notebook fails on import:
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

**Current Development Workflow:**
1. The app uses uv for Python dependency management (not bundled yet)
2. Frontend is React 19 with Vite for hot reload
3. The kernel server (kernel_server.py) is started automatically by Tauri
4. Each project gets its own .venv with ipykernel installed
5. Monaco editor provides the code editing experience

## Design Principles

1. **Notebooks stay normal notebooks** - Edit in Jupyter, VS Code, wherever. Tether just watches and orchestrates.

2. **State is implicit wiring** - No explicit DAG definition. Write `state.get("x")` and `state.set("y")`, dependencies are inferred.

3. **Local-first** - Everything works offline. Cloud is optional sync/backup.

4. **Durable by default** - Every cell checkpoints. Resume from anywhere.

5. **uv handles Python** - No "install Python first" for users. uv bootstraps everything.

6. **Keep it simple** - Use JSX, not TypeScript, unless absolutely necessary. Minimize complexity.

## Future Considerations

- Cloud sync for state (S3/R2)
- Team collaboration (shared state branches)
- Claude Code integration (right-click "Edit with Claude")
- GPU scheduling for heavy cells
- Live output streaming during execution
- Notebook diffing and versioning