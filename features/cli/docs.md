# Workbooks CLI

## Overview

The Workbooks CLI (`workbooks`) provides command-line access to Workbooks projects. It enables:
- Project creation and initialization
- Notebook execution from terminal
- MCP server for Claude Desktop integration
- Project inspection and status checking
- Package management
- State and run history inspection

## Architecture

### Multi-Binary Cargo Project

The CLI is a **separate binary in the same Cargo workspace** as the Tauri app. This allows code sharing while keeping binaries separate.

**`src-tauri/Cargo.toml`:**
```toml
[package]
name = "workbooks"
version = "0.0.1"

[lib]
name = "workbooks_core"
path = "src/lib.rs"

# Tauri GUI app
[[bin]]
name = "workbooks-app"
path = "src/main.rs"

# CLI tool
[[bin]]
name = "workbooks"
path = "src/bin/cli.rs"

[dependencies]
# Shared dependencies
clap = { version = "4.5", features = ["derive"] }
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
# ... other shared deps

# Tauri-specific (only for workbooks-app binary)
[target.'cfg(not(target_os = "ios"))'.dependencies]
tauri = { version = "2.1", features = [] }
# ... other Tauri deps
```

### Shared Library Code

Common functionality lives in `src/lib.rs` and modules:
- `src/project.rs` - Project creation, loading, validation
- `src/engine_http.rs` - Engine server communication
- `src/fs.rs` - File system operations
- `src/config.rs` - Config file management
- `src/python.rs` - UV/Python environment management

Both CLI and GUI import from `workbooks_core`.

## Global Configuration

Workbooks maintains a **global config file** separate from individual project configs. This enables convenient defaults and shared settings across both the CLI and desktop app.

**Location:** `~/.workbooks/config.toml`

**Structure:**
```toml
[global]
default_project = "/Users/you/Projects/my-main-pipeline"
theme = "dark"  # Shared app preference

[recent_projects]
# Auto-populated by app/CLI
paths = [
  "/Users/you/Projects/my-main-pipeline",
  "/Users/you/Projects/data-analysis",
  "/Users/you/Projects/ml-training"
]

[cli]
color = true
verbose = false
```

### Default Project

The global config's `default_project` enables convenient CLI usage without explicit project paths:

**Project Resolution Order (CLI):**
1. Explicit `--project <path>` flag
2. Current directory (if it's a Workbooks project)
3. Global `default_project` from `~/.workbooks/config.toml`
4. Error if none found

**Examples:**
```bash
# Works from anywhere if default project is set
workbooks run notebooks/analysis.ipynb  # Uses default project
workbooks status                        # Shows default project

# Override with explicit flag
workbooks run --project ~/other-project notebooks/test.ipynb

# Works if in a Workbooks project directory
cd ~/Projects/my-pipeline
workbooks run notebooks/test.ipynb  # Uses current directory project
```

**Desktop App Behavior:**
- On launch, auto-open default project (or show welcome screen)
- Welcome screen highlights default project
- "Set as Default Project" button in Project Settings
- Recent projects list shared with CLI

**Managing Default Project:**
```bash
# Set default project
workbooks config set-default /path/to/project

# Show current default
workbooks config get-default

# Remove default (CLI will require --project or cwd)
workbooks config unset-default
```

### Binary Entry Points

**GUI App: `src/main.rs`** (or `src/bin/app.rs`)
```rust
use workbooks_core::*;

fn main() {
    tauri::Builder::default()
        // ... setup
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**CLI: `src/bin/cli.rs`**
```rust
use clap::{Parser, Subcommand};
use workbooks_core::*;

#[derive(Parser)]
#[command(name = "workbooks")]
#[command(about = "Durable workbook orchestration for local-first data pipelines")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init { name: String },
    Run { notebook: Option<String> },
    Mcp { project: String },
    // ... other commands
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init { name } => { /* ... */ }
        Commands::Run { notebook } => { /* ... */ }
        Commands::Mcp { project } => { /* ... */ }
        // ...
    }
}
```

## Installation

### During Tauri App Install

The Tauri installer can optionally install the CLI to system PATH:

**macOS/Linux:**
```bash
# Copy binary to /usr/local/bin
sudo cp workbooks-app.app/Contents/MacOS/workbooks /usr/local/bin/workbooks
sudo chmod +x /usr/local/bin/workbooks
```

**Windows:**
```powershell
# Copy to %LOCALAPPDATA%\Programs\Workbooks\
# Add to PATH in installer
```

**Implementation:**
- Add post-install script to Tauri bundler config
- Prompt user: "Install workbooks CLI to system PATH?"
- Show installation path and verify it's accessible

### Manual Install

```bash
# From source
cd src-tauri
cargo build --release --bin workbooks
sudo cp target/release/workbooks /usr/local/bin/

# Or via installer option (if user skipped during app install)
# Run from Tauri app: Settings → Install CLI
```

### Verification

```bash
workbooks --version
# Output: workbooks 0.1.0
```

## CLI Commands

### `workbooks init <name>`

Create a new Workbooks project.

```bash
workbooks init my-pipeline
cd my-pipeline
```

**Behavior:**
- Creates project directory structure:
  ```
  my-pipeline/
  ├── .workbooks/
  │   └── config.toml
  ├── notebooks/
  ├── pyproject.toml
  ├── uv.lock
  └── My Pipeline.workbooks  # Shortcut file
  ```
- Initializes UV environment (`.venv`)
- Creates default `config.toml`
- Generates `.workbooks` shortcut file for GUI
- Installs `workbooks-core` Python package (for state API)

**Options:**
- `--path <path>` - Create at specific location (default: `./<name>`)
- `--template <template>` - Use template ("blank", "data-analysis", "ml-pipeline")
- `--no-venv` - Skip creating virtual environment

**Example:**
```bash
workbooks init data-pipeline --template data-analysis
```

### `workbooks run [notebook]`

Run a notebook or entire pipeline.

```bash
# Run specific notebook
workbooks run notebooks/load_data.ipynb

# Run all notebooks (inferred dependency order)
workbooks run --all

# Resume interrupted run
workbooks resume
```

**Behavior:**
- Starts engine server if not running
- Executes notebook cells sequentially
- Streams output to terminal
- Creates run history entry with source: "CLI"
- Supports checkpoint/resume if interrupted

**Options:**
- `--all` - Run all notebooks in dependency order
- `--stream` - Stream output in real-time (default: true)
- `--no-checkpoint` - Skip checkpointing (faster, not resumable)
- `--resume [run-id]` - Resume from last checkpoint

**Output:**
```
Running: notebooks/load_data.ipynb
[1/5] Importing libraries... ✓
[2/5] Loading data from S3... ✓ (2.3s)
[3/5] Cleaning data... ✓ (0.8s)
[4/5] Saving to state... ✓
[5/5] Summary statistics... ✓

Run completed in 3.1s
Run ID: run_20250118_143052
```

### `workbooks mcp --project <path>`

Start MCP server for Claude Desktop integration.

```bash
workbooks mcp --project /Users/you/Projects/my-pipeline
```

**Behavior:**
- Validates project path exists and has `.workbooks/` directory
- Starts or connects to existing engine server for project
- Starts FastMCP server on stdio (for Claude Desktop)
- Registers MCP tools with project context
- Runs until Claude Desktop disconnects

**Usage:**
- Called by Claude Desktop (configured in `claude_desktop_config.json`)
- Not typically run manually by users
- Can be run manually for debugging

**Options:**
- `--project <path>` - Required: Path to Workbooks project
- `--debug` - Enable debug logging

**Example Claude Desktop Config:**
```json
{
  "mcpServers": {
    "workbooks-my-pipeline": {
      "command": "workbooks",
      "args": ["mcp", "--project", "/Users/you/Projects/my-pipeline"]
    }
  }
}
```

### `workbooks status`

Show project and pipeline status.

```bash
workbooks status
```

**Output:**
```
Project: my-pipeline
Location: /Users/you/Projects/my-pipeline
Python: 3.11.7 (.venv)
Packages: 23 installed

Notebooks:
  ✓ load_data.ipynb       Last run: 2 hours ago (success)
  ✓ transform.ipynb       Last run: 2 hours ago (success)
  ✗ train_model.ipynb     Last run: 2 hours ago (failed)

Recent runs:
  run_20250118_143052  transform.ipynb    success  3.1s
  run_20250118_141230  train_model.ipynb  failed   12.5s
  run_20250118_135510  load_data.ipynb    success  8.2s

Engine: Running (port 8765)
```

**Options:**
- `--verbose` - Show detailed status (cell counts, state variables, etc.)
- `--json` - Output as JSON for scripting

### `workbooks state <subcommand>`

Manage state system (future feature, once state is implemented).

```bash
# List state variables
workbooks state list

# Inspect a state variable
workbooks state inspect customers

# Delete a state variable
workbooks state delete customers

# Fork state to new branch
workbooks state fork experiment-1

# Switch to state branch
workbooks state switch experiment-1
```

### `workbooks config <subcommand>`

Manage global configuration.

```bash
# Set default project
workbooks config set-default /path/to/project
workbooks config set-default .  # Use current directory

# Show current default project
workbooks config get-default
# Output: /Users/you/Projects/my-pipeline

# Remove default project
workbooks config unset-default

# Show all global config
workbooks config show

# Edit config file directly
workbooks config edit  # Opens in $EDITOR
```

**Behavior:**
- Reads/writes `~/.workbooks/config.toml`
- Validates project paths exist
- Creates config file if doesn't exist
- Provides helpful error messages

**Use Cases:**
- Set default project for convenient CLI usage
- Share config between CLI and desktop app
- Manage recent projects list

### `workbooks logs [notebook]`

View execution logs.

```bash
# View all recent logs
workbooks logs

# View logs for specific notebook
workbooks logs notebooks/train_model.ipynb

# View logs for specific run
workbooks logs --run run_20250118_141230

# Follow logs in real-time
workbooks logs --follow
```

**Output:**
```
[2025-01-18 14:12:30] [train_model.ipynb] Starting execution
[2025-01-18 14:12:31] [train_model.ipynb] Cell 1: Importing libraries
[2025-01-18 14:12:32] [train_model.ipynb] Cell 2: Loading data from state
[2025-01-18 14:12:33] [train_model.ipynb] ERROR: KeyError: 'customers'
[2025-01-18 14:12:33] [train_model.ipynb] Execution failed
```

### `workbooks resume [notebook|run-id]`

Resume interrupted execution.

```bash
# Resume last interrupted run
workbooks resume

# Resume specific notebook
workbooks resume notebooks/train_model.ipynb

# Resume specific run ID
workbooks resume run_20250118_141230
```

**Behavior:**
- Loads last checkpoint
- Continues from next cell
- Validates code hasn't changed (hash chain)
- Warns if code changed since checkpoint

### `workbooks schedule <subcommand>`

Manage scheduled runs (future feature).

```bash
# Schedule a notebook
workbooks schedule add notebooks/daily_report.ipynb --cron "0 9 * * *"

# List schedules
workbooks schedule list

# Disable a schedule
workbooks schedule disable daily_report

# View schedule history
workbooks schedule history daily_report
```

### `workbooks open [path]`

Open project in Workbooks GUI app.

```bash
# Open current project
workbooks open

# Open specific project
workbooks open /path/to/project

# Open specific notebook in GUI
workbooks open notebooks/analysis.ipynb
```

**Behavior:**
- Launches Tauri app with project loaded
- If app already running, opens in new tab
- Uses `.workbooks` shortcut file format

### `workbooks install-cli`

Install CLI to system PATH (helper command).

```bash
workbooks install-cli
```

**Behavior:**
- Called from Tauri app: Settings → Install CLI
- Copies binary to system PATH location
- Verifies installation
- Shows success message with path

This can be used if the user skipped CLI installation during app setup.

### `workbooks version`

Show version information.

```bash
workbooks version
```

**Output:**
```
workbooks 0.1.0
workbooks-app 0.1.0
engine-server 0.1.0
Python: 3.11.7
UV: 0.5.0
```

### `workbooks doctor`

Diagnose installation and configuration issues.

```bash
workbooks doctor
```

**Output:**
```
Checking Workbooks installation...
✓ workbooks CLI installed
✓ Tauri app installed
✓ UV available
✓ Python 3.11+ available
✗ Project .workbooks/ directory missing
  Fix: Run 'workbooks init <name>' to create a project

Engine server: ✓ Running on port 8765
Claude Desktop config: ✓ Found at ~/Library/Application Support/Claude/
  - workbooks-my-pipeline: ✓ Configured correctly
```

## CLI Architecture Benefits

### Code Sharing
- **Project management** - Same code for init, loading, validation
- **Engine communication** - Shared HTTP client for engine server
- **Config handling** - Single source of truth for `.workbooks/config.toml`
- **File operations** - Consistent file system abstractions

### Distribution
- **Single installer** - Tauri app installer can optionally install CLI
- **Version sync** - CLI and GUI always have matching versions
- **Reduced maintenance** - No separate release process

### Development Workflow
- **Shared testing** - Test logic once, use in both CLI and GUI
- **Consistent behavior** - Project operations work identically
- **Easier debugging** - CLI can test features without GUI

## Implementation Notes

### Clap for CLI Parsing
Use `clap` with derive macros for clean command definitions:

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "workbooks")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Durable workbook orchestration")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init {
        name: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        template: Option<String>,
    },
    Run {
        notebook: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        stream: bool,
    },
    // ... other commands
}
```

### Async Runtime
CLI needs async runtime for HTTP requests, engine communication:

```rust
#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run { notebook, .. } => {
            run_notebook(notebook).await.unwrap();
        }
        // ...
    }
}
```

### Output Formatting
- Use `indicatif` for progress bars
- Use `colored` for colored terminal output
- Support `--json` flag for machine-readable output
- Support `--quiet` flag for minimal output

### Error Handling
- Clear error messages with suggestions
- Exit codes:
  - `0` - Success
  - `1` - General error
  - `2` - Config error
  - `3` - Execution failed
  - `4` - Not found (project, notebook, etc.)

## Future Enhancements

- **Shell completions** - Generate for bash, zsh, fish
- **Interactive mode** - `workbooks shell` for REPL-like experience
- **Notebook diffing** - `workbooks diff notebook.ipynb`
- **Export/import** - `workbooks export`, `workbooks import`
- **Remote execution** - `workbooks run --remote` (cloud execution)
- **Watch mode** - `workbooks watch` (auto-run on file changes)
