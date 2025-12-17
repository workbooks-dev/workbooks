# Tether

Durable notebook orchestration for local-first data pipelines.

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
- **React** + **TypeScript** - frontend
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

## Key Files to Build

### Rust (src-tauri/)
- `src/main.rs` - Tauri app entry, command handlers
- `src/python.rs` - uv integration, environment management
- `src/state.rs` - State management, forking
- `src/executor.rs` - Notebook execution orchestration
- `src/scheduler.rs` - Cron-based scheduling

### TypeScript (src/)
- `App.tsx` - Main app layout
- `components/Canvas.tsx` - React Flow notebook visualization
- `components/StatePanel.tsx` - State variable inspector
- `components/NotebookList.tsx` - Sidebar notebook list
- `components/RunLog.tsx` - Execution logs viewer
- `hooks/useProject.ts` - Project state management
- `hooks/useTether.ts` - Tauri command bindings

### Python (tether-core/)
- `tether/__init__.py` - Public API (state)
- `tether/state.py` - State get/set/list/delete
- `tether/executor.py` - Cell-by-cell execution with checkpointing
- `tether/checkpoint.py` - Namespace serialization
- `tether/cli.py` - CLI entry point

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
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Node
# (use your preferred method)

# Install Tauri CLI
cargo install tauri-cli

# Clone and setup
git clone <repo>
cd tether

# Install frontend deps
npm install

# Fetch uv binaries for all platforms
python scripts/fetch_uv.py

# Run in dev mode
cargo tauri dev

# Build for production
cargo tauri build
```

## Design Principles

1. **Notebooks stay normal notebooks** - Edit in Jupyter, VS Code, wherever. Tether just watches and orchestrates.

2. **State is implicit wiring** - No explicit DAG definition. Write `state.get("x")` and `state.set("y")`, dependencies are inferred.

3. **Local-first** - Everything works offline. Cloud is optional sync/backup.

4. **Durable by default** - Every cell checkpoints. Resume from anywhere.

5. **uv handles Python** - No "install Python first" for users. uv bootstraps everything.

## Future Considerations

- Cloud sync for state (S3/R2)
- Team collaboration (shared state branches)
- Claude Code integration (right-click "Edit with Claude")
- GPU scheduling for heavy cells
- Live output streaming during execution
- Notebook diffing and versioning