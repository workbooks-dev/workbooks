# CLI - Todo

## Phase 0: Global Configuration System

### Global Config Infrastructure
- [ ] Create `~/.tether/` directory on first run
- [ ] Implement global config handling in `src/config.rs`
  - [ ] `GlobalConfig` struct matching TOML structure
  - [ ] `load_global_config()` - Read from `~/.tether/config.toml`
  - [ ] `save_global_config()` - Write to `~/.tether/config.toml`
  - [ ] Create default config if doesn't exist
  - [ ] Handle malformed config gracefully
- [ ] Implement project resolution logic
  - [ ] Check `--project` flag first
  - [ ] Check if current directory is Tether project
  - [ ] Check global `default_project`
  - [ ] Return clear error if none found
- [ ] Add recent projects tracking
  - [ ] Update `recent_projects` array on project access
  - [ ] Limit to 10 most recent
  - [ ] Shared between CLI and app

### `tether config` Subcommand
- [ ] Implement `config set-default <path>`
  - [ ] Validate path exists and is Tether project
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
  - [ ] Open `~/.tether/config.toml` in $EDITOR
  - [ ] Validate config after editing
  - [ ] Show errors if malformed

## Phase 1: Multi-Binary Setup

### Cargo Configuration
- [ ] Refactor `src-tauri/Cargo.toml` to support multiple binaries
  - [ ] Define `[lib]` section for shared code
  - [ ] Define `[[bin]]` for tether-app (GUI)
  - [ ] Define `[[bin]]` for tether (CLI)
  - [ ] Add CLI dependencies (clap, indicatif, colored)
- [ ] Move shared code to `src/lib.rs`
  - [ ] Export project management functions
  - [ ] Export engine communication functions
  - [ ] Export config handling
  - [ ] Export file system operations
- [ ] Create `src/bin/cli.rs` entry point
  - [ ] Setup clap parser with basic structure
  - [ ] Add version, about, author metadata
  - [ ] Setup tokio async runtime
- [ ] Update `src/main.rs` to import from `tether_core` lib
- [ ] Test both binaries build successfully

## Phase 2: Core CLI Commands

### `tether init`
- [ ] Implement `init` subcommand
  - [ ] Accept project name and options (--path, --template)
  - [ ] Create directory structure (.tether/, notebooks/)
  - [ ] Generate config.toml with defaults
  - [ ] Create pyproject.toml and uv.lock
  - [ ] Initialize UV virtual environment
  - [ ] Install tether-core Python package
  - [ ] Create .tether shortcut file
  - [ ] Print success message with next steps
- [ ] Add templates support
  - [ ] "blank" template (minimal setup)
  - [ ] "data-analysis" template (common data science packages)
  - [ ] "ml-pipeline" template (ML/training packages)
- [ ] Error handling
  - [ ] Directory already exists
  - [ ] UV not available
  - [ ] Python version incompatible

### `tether run`
- [ ] Implement `run` subcommand
  - [ ] Parse notebook path argument
  - [ ] Detect current project (from cwd or config)
  - [ ] Start/connect to engine server
  - [ ] Execute notebook via HTTP endpoint
  - [ ] Stream output to terminal with progress indicators
  - [ ] Handle execution errors gracefully
  - [ ] Create run history entry with source: "CLI"
  - [ ] Display run summary (time, status, run ID)
- [ ] Add `--all` flag for full pipeline execution
  - [ ] Find all notebooks in project
  - [ ] Infer dependency order (future: use state deps)
  - [ ] Execute in sequence
- [ ] Add `--stream` flag (default: true)
  - [ ] Real-time output streaming
  - [ ] Progress bars for long-running cells
- [ ] Add `--no-checkpoint` flag
  - [ ] Skip checkpointing for faster execution
  - [ ] Warn that run is not resumable

### `tether mcp`
- [ ] Implement `mcp` subcommand
  - [ ] Require `--project <path>` argument
  - [ ] Validate project path exists
  - [ ] Validate .tether/ directory exists
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

### `tether status`
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

### `tether logs`
- [ ] Implement `logs` subcommand
  - [ ] Read from .tether/runs/ directory
  - [ ] Parse and display logs
  - [ ] Support filtering by notebook
  - [ ] Support filtering by run ID
  - [ ] Format with timestamps and colors
- [ ] Add `--run <run-id>` flag
- [ ] Add `--follow` flag for real-time tailing
- [ ] Add `--level <level>` flag (info, warning, error)

### `tether version`
- [ ] Implement `version` subcommand
  - [ ] Display tether CLI version
  - [ ] Display tether-app version
  - [ ] Display engine-server version
  - [ ] Display Python version
  - [ ] Display UV version
- [ ] Format as simple text or detailed table

### `tether doctor`
- [ ] Implement `doctor` subcommand
  - [ ] Check tether CLI is accessible
  - [ ] Check Tauri app is installed
  - [ ] Check UV is available and version
  - [ ] Check Python version compatibility
  - [ ] Check current project structure (if in project)
  - [ ] Check engine server status
  - [ ] Check Claude Desktop config (if exists)
  - [ ] Validate project config.toml
- [ ] Provide fix suggestions for each issue
- [ ] Color-code results (✓ green, ✗ red, ⚠ yellow)

## Phase 3: Installation & Distribution

### CLI Installation
- [ ] Add Tauri installer post-install script
  - [ ] Detect OS (macOS, Windows, Linux)
  - [ ] Prompt user: "Install tether CLI to system PATH?"
  - [ ] Copy binary to appropriate location:
    - [ ] macOS/Linux: `/usr/local/bin/tether`
    - [ ] Windows: `%LOCALAPPDATA%\Programs\Tether\bin\`
  - [ ] Set executable permissions (Unix)
  - [ ] Add to PATH (Windows registry)
  - [ ] Verify installation with `tether --version`
  - [ ] Show success message with installation path

### Manual Installation Helper
- [ ] Implement `tether install-cli` helper command
  - [ ] Can be run from bundled app binary
  - [ ] Same logic as installer post-install
  - [ ] Used if user skipped during app install
- [ ] Add "Install CLI" button in Tauri app settings
  - [ ] Calls bundled binary's install-cli function
  - [ ] Shows installation progress
  - [ ] Verifies success

### Build & Release
- [ ] Update build scripts to compile both binaries
  - [ ] `cargo build --bin tether-app`
  - [ ] `cargo build --bin tether`
- [ ] Package CLI binary with Tauri app bundle
- [ ] Add CLI binary to GitHub releases
- [ ] Test installation on all platforms (macOS, Windows, Linux)

## Phase 4: Advanced CLI Commands (Future)

### `tether resume`
- [ ] Implement resume functionality
  - [ ] Find last interrupted run
  - [ ] Load checkpoint from .tether/runs/{run-id}/
  - [ ] Restore namespace
  - [ ] Continue from next cell
  - [ ] Validate code hasn't changed (hash chain)
- [ ] Support `--notebook <path>` to resume specific notebook
- [ ] Support `--run <run-id>` to resume specific run

### `tether state` (when state system is built)
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

### `tether schedule` (when scheduling is built)
- [ ] Implement `schedule add` subcommand
  - [ ] Accept notebook path and cron expression
  - [ ] Validate cron syntax
  - [ ] Store in .tether/schedules/
  - [ ] Show next run time
- [ ] Implement `schedule list`
  - [ ] Display all schedules
  - [ ] Show enabled/disabled status
  - [ ] Show next run times
- [ ] Implement `schedule disable <name>`
- [ ] Implement `schedule enable <name>`
- [ ] Implement `schedule remove <name>`
- [ ] Implement `schedule history <name>`
  - [ ] Show run history for scheduled notebook

### `tether open`
- [ ] Implement `open` subcommand
  - [ ] Detect current project or accept path
  - [ ] Launch Tauri app with project loaded
  - [ ] Use OS-specific app launching
  - [ ] Support opening specific notebook in app
- [ ] Check if app is already running
  - [ ] Send IPC message to open in new tab
  - [ ] Or launch new instance

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
  - [ ] "Project not found" → "Run 'tether init' to create one"
  - [ ] "UV not found" → "Install UV: https://..."
  - [ ] "Notebook not found" → "Available notebooks: ..."
- [ ] Show error codes for debugging
- [ ] Link to documentation for complex errors

### Shell Completions
- [ ] Generate completions with clap
- [ ] Support bash completion
- [ ] Support zsh completion
- [ ] Support fish completion
- [ ] Add `tether completions <shell>` command
- [ ] Document installation in README

### Help & Documentation
- [ ] Improve `--help` text for all commands
- [ ] Add examples to help text
- [ ] Add `tether help <command>` alias
- [ ] Create man pages (Unix)
- [ ] Link to online docs from CLI

## Phase 6: Testing

### Unit Tests
- [ ] Test project creation logic
- [ ] Test config parsing and validation
- [ ] Test engine communication
- [ ] Test error handling

### Integration Tests
- [ ] Test full `tether init` flow
- [ ] Test `tether run` execution
- [ ] Test `tether mcp` server startup
- [ ] Test CLI installation process
- [ ] Test multi-platform compatibility

### CI/CD
- [ ] Add CLI build to CI pipeline
- [ ] Test CLI on all platforms
- [ ] Generate release artifacts (binaries)
- [ ] Automate version bumping
