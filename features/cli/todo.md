# CLI - Todo

## Overview

The CLI serves three primary purposes:
1. **MCP Integration** - Enable Claude Desktop and other MCP clients to interact with Workbooks projects
2. **Automation** - Schedule and run workbooks from scripts, cron, CI/CD
3. **Quick Actions** - Open projects (`workbooks .`), check status, view logs

**Installation Philosophy:**
- **Desktop app is 100% primary interface** - Non-technical users never need to touch CLI
- **Silent install by default** - CLI automatically added to PATH during app installation
- **No prompts or friction** - Installation happens in background
- **CLI is there when needed** - For automation, MCP, power users, sharing commands
- **Target audience:** Finance, marketing, accounting professionals doing analysis (not developers)

## Phase 0: Global Configuration System

### Global Config Infrastructure
- [ ] Create `~/.workbooks/` directory on first run
- [ ] Implement global config handling in `src/config.rs`
  - [ ] `GlobalConfig` struct matching TOML structure
  - [ ] `load_global_config()` - Read from `~/.workbooks/config.toml`
  - [ ] `save_global_config()` - Write to `~/.workbooks/config.toml`
  - [ ] Create default config if doesn't exist
  - [ ] Handle malformed config gracefully
- [ ] Implement project resolution logic
  - [ ] Check `--project` flag first
  - [ ] Check if current directory is Workbooks project
  - [ ] Check global `default_project`
  - [ ] Return clear error if none found
- [ ] Add recent projects tracking
  - [ ] Update `recent_projects` array on project access
  - [ ] Limit to 10 most recent
  - [ ] Shared between CLI and app

### `workbooks config` Subcommand
- [ ] Implement `config set-default <path>`
  - [ ] Validate path exists and is Workbooks project
  - [ ] Update `global.default_project` in config
  - [ ] Show success message with path
  - [ ] Handle relative paths (convert to absolute)
- [ ] Implement `config get-default`
  - [ ] Read and display current default project
  - [ ] Show "(none set)" if empty
  - [ ] Exit code 1 if not set (for scripting)
- [ ] Implement `config unset-default`
  - [ ] Remove `global.default_project` from config
  - [ ] Show confirmation message
- [ ] Implement `config show`
  - [ ] Display formatted global config
  - [ ] Show default project, recent projects, CLI settings
  - [ ] Support `--json` flag
- [ ] Implement `config edit`
  - [ ] Open `~/.workbooks/config.toml` in $EDITOR
  - [ ] Validate config after editing
  - [ ] Show errors if malformed

### `workbooks run` subcommand ✅ CORE COMPLETED
- [x] Implement `run <path-to-ipynb>`
  - [x] Automatically load a project by walking up to find `.workbooks` directory
  - [x] Falls back to basic mode if no project found
  - [x] Runs entire notebook just like "Run all" in the app UI
  - [x] Automatically installs ipykernel if needed
  - [x] Handles pyproject.toml with or without workbooks dependency group
- [ ] Secrets integration
  - [ ] Load secrets from project's SecretsManager if available
  - [ ] Inject secrets as environment variables into engine
- [ ] Enhanced UX features
  - [ ] Prompt to create project if not available
  - [ ] Prompt to load `.env`/`.env.local` into project's secrets
  - [ ] Add progress indicators during cell execution
  - [ ] Add `--verbose` flag for detailed output
  - [ ] Add `--quiet` flag for minimal output


## Phase 1: Multi-Binary Setup ✅ COMPLETED

### Cargo Configuration
- [x] Refactored `src-tauri/Cargo.toml` to support multiple binaries
  - [x] Define `[lib]` section for shared code (workbooks_lib)
  - [x] Define `[[bin]]` for workbooks-gui (GUI) at `src/main.rs`
  - [x] Define `[[bin]]` for workbooks (CLI) at `src/cli.rs`
  - [x] CLI dependencies already present (clap, tokio, anyhow)
- [x] Made core modules public in `src/lib.rs`
  - [x] `pub mod python` - Environment management
  - [x] `pub mod project` - Project loading/creation
  - [x] `pub mod engine_http` - Engine communication
  - [x] `pub mod scheduler` - Scheduler management
- [x] Created CLI at `src/cli.rs`
  - [x] Setup clap parser with Commands enum
  - [x] Added Run and Schedule subcommands
  - [x] Setup tokio async runtime
- [x] Both binaries build successfully

## Phase 2: Core CLI Commands

### `workbooks init`
- [ ] Implement `init` subcommand
  - [ ] Accept project name and options (--path, --template)
  - [ ] Create directory structure (.workbooks/)
  - [ ] Generate config.toml with defaults
  - [ ] Create pyproject.toml and uv.lock
  - [ ] Initialize UV virtual environment
  - [ ] Install workbooks-core Python package
  - [ ] Create .workbooks shortcut file
  - [ ] Print success message with next steps
- [ ] Add templates support
  - [ ] "blank" template (minimal setup)
  - [ ] "data-analysis" template (common data science packages)
  - [ ] "ml-pipeline" template (ML/training packages)
- [ ] Error handling
  - [ ] Directory already exists
  - [ ] UV not available
  - [ ] Python version incompatible

### `workbooks run` ✅ CORE COMPLETED
- [x] Implement basic `run` subcommand
  - [x] Parse notebook path argument
  - [x] Auto-detect project by walking up to find `.workbooks`
  - [x] Start engine server on available port
  - [x] Execute notebook via `/engine/execute-all` HTTP endpoint
  - [x] Display execution results in terminal
  - [x] Handle execution errors gracefully
  - [x] Display run summary (cells, success/failed)
- [ ] Enhanced features
  - [ ] Create run history entry with source: "CLI"
  - [ ] Display execution time and run ID
  - [ ] Add `--all` flag for full pipeline execution
    - [ ] Find all notebooks in project
    - [ ] Infer dependency order (future: use state deps)
    - [ ] Execute in sequence
  - [ ] Add `--stream` flag (default: true)
    - [ ] Real-time output streaming (currently non-streaming)
    - [ ] Progress bars for long-running cells
  - [ ] Add `--no-checkpoint` flag
    - [ ] Skip checkpointing for faster execution
    - [ ] Warn that run is not resumable

### `workbooks mcp` (PRIMARY USE CASE)
**This is a primary driver for CLI - enables Claude Desktop integration**

- [ ] Implement `mcp` subcommand
  - [ ] Require `--project <path>` argument
  - [ ] Validate project path exists
  - [ ] Validate .workbooks/ directory exists
  - [ ] Load project config
  - [ ] Start/connect to engine server
  - [ ] Start FastMCP server on stdio
  - [ ] Register all MCP tools with project context
  - [ ] Run event loop until disconnect
- [ ] Add `--debug` flag for verbose logging
- [ ] Error handling
  - [ ] Project not found
  - [ ] Config malformed
  - [ ] Engine server failed to start
  - [ ] MCP protocol errors
- [ ] Documentation
  - [ ] Add to Claude Desktop config examples
  - [ ] Document available MCP tools
  - [ ] Provide troubleshooting guide

### `workbooks status`
- [ ] Implement `status` subcommand
  - [ ] Detect current project
  - [ ] Load project config
  - [ ] Display project name and location
  - [ ] Display Python version and .venv path
  - [ ] List installed packages (from pyproject.toml)
  - [ ] List notebooks with last run info
  - [ ] Show recent runs (last 5)
  - [ ] Check engine server status
- [ ] Add `--verbose` flag for detailed info
  - [ ] Cell counts per notebook
  - [ ] State variables (future)
  - [ ] Disk usage
- [ ] Add `--json` flag for machine-readable output
- [ ] Format output with colors and icons (✓, ✗, ⚠)

### `workbooks logs`
- [ ] Implement `logs` subcommand
  - [ ] Read from .workbooks/runs/ directory
  - [ ] Parse and display logs
  - [ ] Support filtering by notebook
  - [ ] Support filtering by run ID
  - [ ] Format with timestamps and colors
- [ ] Add `--run <run-id>` flag
- [ ] Add `--follow` flag for real-time tailing
- [ ] Add `--level <level>` flag (info, warning, error)

### `workbooks version`
- [ ] Implement `version` subcommand
  - [ ] Display workbooks CLI version
  - [ ] Display workbooks-app version
  - [ ] Display engine-server version
  - [ ] Display Python version
  - [ ] Display UV version
- [ ] Format as simple text or detailed table

### `workbooks doctor`
**Critical for debugging silent installation issues**

- [ ] Implement `doctor` subcommand
  - [ ] Check workbooks CLI is accessible and in PATH
  - [ ] Check Tauri app is installed and location
  - [ ] Check UV is available and version
  - [ ] Check Python version compatibility
  - [ ] Check current project structure (if in project)
  - [ ] Check engine server status
  - [ ] Check Claude Desktop config (if exists)
  - [ ] Validate project config.toml (if in project)
  - [ ] Verify CLI installation location and permissions
- [ ] Provide fix suggestions for each issue
  - [ ] "CLI not in PATH" → Show manual PATH addition instructions
  - [ ] "App not installed" → Provide download link
  - [ ] "UV not found" → Offer to run bundled UV or provide install link
- [ ] Color-code results (✓ green, ✗ red, ⚠ yellow)
- [ ] Add `--verbose` flag for detailed diagnostics
- [ ] Add `--fix` flag to automatically repair common issues

## Phase 3: Installation & Distribution ✅ CORE COMPLETED

### Silent CLI Installation ✅ IMPLEMENTED
**Philosophy: Zero friction, happens automatically on first app launch**

- [x] Automatic installation on first app launch
  - [x] Detect OS (macOS, Windows, Linux)
  - [x] **Automatically** copy binary to appropriate location (no prompts):
    - [x] macOS/Linux: `~/.local/bin/workbooks`
    - [x] Windows: `%LOCALAPPDATA%\Programs\Workbooks\bin\`
  - [x] Set executable permissions (Unix)
  - [x] Add to PATH automatically:
    - [x] macOS/Linux: Append to `.zshrc` / `.bashrc` if not already present
    - [x] Check for existing PATH entry to avoid duplicates
  - [x] Frontend hook checks CLI on app startup
  - [x] Installs silently in background if not found
  - [x] Logs status to browser console
- [ ] Windows PATH modification (not yet implemented)
  - [ ] Update user PATH environment variable (no admin required)
  - [ ] Verify PATH update works on Windows

### Installation Commands ✅ IMPLEMENTED
- [x] `install_cli` Tauri command
  - [x] Copies CLI binary from app bundle to system PATH
  - [x] Smart path detection (checks multiple bundle locations)
  - [x] Automatic PATH updating on Unix systems
- [x] `check_cli_installed` Tauri command
  - [x] Checks if `workbooks` is in PATH using `which`
- [x] `get_path_instructions` Tauri command
  - [x] Returns shell-specific instructions for manual PATH setup

### Build & Release
- [x] Update build scripts to compile both binaries
  - [x] Added `npm run prebuild:cli` script
  - [x] `app:build` now compiles CLI before bundling app
  - [x] CLI built in release mode for production
- [x] Package CLI binary with Tauri app bundle
  - [x] Included in app bundle resources via tauri.conf.json
- [x] Automatic CLI updates ✅ COMPLETED
  - [x] Version detection (compares installed vs bundled)
  - [x] Auto-update on app launch
  - [x] Silent updates in background
  - [x] CLI stays in sync with app version
- [ ] Code signing
  - [ ] macOS: Sign CLI binary for Gatekeeper
  - [ ] Windows: Code sign CLI binary
- [ ] Add standalone CLI binary to GitHub releases
  - [ ] For users who want CLI without desktop app
  - [ ] For CI/CD environments and servers
  - [ ] Provide installation script: `curl -sSf https://workbooks.dev/install.sh | sh`
- [ ] Test installation on all platforms
  - [ ] macOS (Intel + Apple Silicon)
  - [ ] Windows (x64)
  - [ ] Linux (x64, ARM64)
  - [ ] Verify PATH modification works correctly
  - [ ] Verify silent install shows no prompts
  - [ ] Test on fresh machines without prior installations

## Phase 4: Advanced CLI Commands (Future)

### `workbooks resume`
- [ ] Implement resume functionality
  - [ ] Find last interrupted run
  - [ ] Load checkpoint from .workbooks/runs/{run-id}/
  - [ ] Restore namespace
  - [ ] Continue from next cell
  - [ ] Validate code hasn't changed (hash chain)
- [ ] Support `--notebook <path>` to resume specific notebook
- [ ] Support `--run <run-id>` to resume specific run

### `workbooks state` (when state system is built)
- [ ] Implement `state list` subcommand
  - [ ] Query state.db
  - [ ] Display variable names, types, sizes
  - [ ] Show last updated timestamp
- [ ] Implement `state inspect <key>`
  - [ ] Load variable from blob storage
  - [ ] Display summary (shape, columns, etc.)
  - [ ] Support `--full` to show complete data
- [ ] Implement `state delete <key>`
  - [ ] Remove from state.db
  - [ ] Remove blob file
  - [ ] Confirm before deletion
- [ ] Implement `state fork <branch>`
  - [ ] Copy state.db to branches/
  - [ ] Copy blob directory
  - [ ] Update config to track branches
- [ ] Implement `state switch <branch>`
  - [ ] Swap state.db files
  - [ ] Update blob symlinks

### `workbooks schedule` ✅ BASIC COMPLETED
- [x] Implement `schedule add` subcommand
  - [x] Accept notebook path and cron expression
  - [x] Support `--cron "expression"` for custom schedules
  - [x] Support preset flags: `--daily`, `--hourly`, `--weekly`
  - [x] Validate cron syntax
  - [x] Store in SchedulerManager (SQLite backend)
  - [x] Show next run time
  - [x] Accept optional `--project <path>` flag
- [x] Implement `schedule list`
  - [x] Display all schedules
  - [x] Show enabled/disabled status
  - [x] Show next run times
  - [x] Display workbook path, project, cron expression
- [x] Implement `schedule remove <id>`
  - [x] Delete schedule by ID
  - [x] Show confirmation message
- [ ] Additional features
  - [ ] Implement `schedule disable <id>`
  - [ ] Implement `schedule enable <id>`
  - [ ] Implement `schedule history <id>`
    - [ ] Show run history for scheduled notebook

### `workbooks open` / `workbooks .` (Quick Launch)
**Primary way to open projects from terminal**

- [ ] Implement `open` subcommand with variants:
  - [ ] `workbooks .` - Open current directory as project (most common usage)
  - [ ] `workbooks <path>` - Open specific project path
  - [ ] `workbooks` (no args) - Open app to welcome screen (or show help)
  - [ ] `workbooks open <path>` - Explicit form (same as `workbooks <path>`)
  - [ ] Validate path is a Workbooks project (has `.workbooks/` directory)
  - [ ] Show helpful error if not a project: "Not a Workbooks project. Run 'workbooks init' to create one."
- [ ] Launch behavior
  - [ ] Launch Tauri app with project loaded
  - [ ] Use OS-specific app launching:
    - [ ] macOS: Use `open` command or NSWorkspace
    - [ ] Windows: Use ShellExecute or registry associations
    - [ ] Linux: Use `xdg-open` or desktop entry
  - [ ] Pass project path as command-line argument to app
  - [ ] Support deep-linking to specific notebook: `workbooks . --notebook analysis.ipynb`
- [ ] Smart instance management
  - [ ] Check if app is already running
  - [ ] If running: Send IPC message to open project in new tab
  - [ ] If not running: Launch new instance with project
  - [ ] Add `--new-window` flag to force new instance

## Phase 5: CLI UX Improvements

### Output Formatting
- [ ] Add colored output with `colored` crate
  - [ ] Success messages in green
  - [ ] Errors in red
  - [ ] Warnings in yellow
  - [ ] Info in blue
- [ ] Add progress bars with `indicatif`
  - [ ] Cell execution progress
  - [ ] Package installation progress
  - [ ] File operations progress
- [ ] Add spinners for long operations
- [ ] Support `--quiet` flag for minimal output
- [ ] Support `--json` flag for machine-readable output

### Error Messages
- [ ] Standardize error message format
- [ ] Include helpful suggestions for common errors
  - [ ] "Project not found" → "Run 'workbooks init' to create one"
  - [ ] "UV not found" → "Install UV: https://..."
  - [ ] "Notebook not found" → "Available notebooks: ..."
- [ ] Show error codes for debugging
- [ ] Link to documentation for complex errors

### Shell Completions
- [ ] Generate completions with clap
- [ ] Support bash completion
- [ ] Support zsh completion
- [ ] Support fish completion
- [ ] Add `workbooks completions <shell>` command
- [ ] Document installation in README

### Help & Documentation
- [ ] Improve `--help` text for all commands
- [ ] Add examples to help text
- [ ] Add `workbooks help <command>` alias
- [ ] Create man pages (Unix)
- [ ] Link to online docs from CLI

## Phase 6: Testing

### Unit Tests
- [ ] Test project creation logic
- [ ] Test config parsing and validation
- [ ] Test engine communication
- [ ] Test error handling

### Integration Tests
- [ ] Test full `workbooks init` flow
- [ ] Test `workbooks run` execution
- [ ] Test `workbooks mcp` server startup
- [ ] Test CLI installation process
- [ ] Test multi-platform compatibility

### CI/CD
- [ ] Add CLI build to CI pipeline
- [ ] Test CLI on all platforms
- [ ] Generate release artifacts (binaries)
- [ ] Automate version bumping
