# CLI - Done

## Design Phase
- [x] Determined architecture: multi-binary Cargo project
- [x] Designed shared library structure for code reuse
- [x] Planned CLI command structure and subcommands
- [x] Created feature documentation structure
- [x] Designed global configuration system (`~/.tether/config.toml`)
- [x] Designed default project feature (shared with app)
- [x] Designed project resolution logic (flag → cwd → default)
- [x] Planned `tether config` subcommand for managing global settings

## Phase 0: Core Implementation

### Multi-Binary Setup
- [x] Configured `Cargo.toml` with separate binaries
  - [x] `[[bin]]` for tether (CLI) at `src/cli.rs`
  - [x] `[[bin]]` for tether-gui (GUI) at `src/main.rs`
  - [x] `[lib]` for shared code at `src/lib.rs` (named `tether_lib`)
- [x] Made core modules public for CLI access
  - [x] `pub mod python` - Python/UV environment management
  - [x] `pub mod project` - Project loading and creation
  - [x] `pub mod engine_http` - Engine server HTTP communication
  - [x] `pub mod scheduler` - Scheduler management

### `tether run` Implementation
- [x] Implemented basic `run <workbook>` command structure with clap
- [x] Added automatic project detection
  - [x] Walks up directory tree from workbook location to find `.tether` directory
  - [x] Falls back to workbook's parent if no `.tether` found
  - [x] Accepts optional `--project <path>` flag to override
- [x] Implemented project loading logic
  - [x] Loads existing Tether projects via `project::load_project()`
  - [x] Creates minimal project info if not a Tether project (basic mode)
  - [x] Displays warning when running in basic mode
- [x] Added Python environment setup
  - [x] Ensures venv exists for project
  - [x] Syncs dependencies from `pyproject.toml` if present
- [x] Implemented notebook execution
  - [x] Parses `.ipynb` file and extracts cells
  - [x] Starts engine server on available port
  - [x] Initializes engine for the workbook
  - [x] Calls `/engine/execute-all` endpoint to run all cells
  - [x] Displays execution progress and results
  - [x] Shows cell outputs (stdout, stderr, results, errors)
  - [x] Cleanly shuts down engine after execution
- [x] Added execution result handling
  - [x] Displays summary (total cells, successful, failed)
  - [x] Shows individual cell results with ✓/✗ indicators
  - [x] Prints stdout/stderr outputs to terminal
  - [x] Displays error tracebacks for failed cells
  - [x] Exits with error code if execution failed

### Engine HTTP Extensions
- [x] Added `execute_all_http()` function to `engine_http.rs`
  - [x] Calls POST `/engine/execute-all` endpoint
  - [x] Sends notebook cells as JSON payload
  - [x] Returns `ExecuteAllResponse` with cell results
- [x] Added supporting types:
  - [x] `Cell` - Cell source and type for request
  - [x] `CellExecutionResult` - Individual cell result
  - [x] `ExecuteAllResponse` - Complete execution response

### `tether schedule` Implementation
- [x] Implemented `schedule add` subcommand
  - [x] Accepts workbook path and cron expression/presets
  - [x] Supports `--cron "expression"` for custom schedules
  - [x] Supports `--daily`, `--hourly`, `--weekly` presets
  - [x] Canonicalizes workbook and project paths
  - [x] Stores schedule in SchedulerManager
  - [x] Displays confirmation with schedule details
- [x] Implemented `schedule list` subcommand
  - [x] Shows all scheduled workbooks
  - [x] Displays ID, workbook path, project, cron expression
  - [x] Shows enabled status and next run time
- [x] Implemented `schedule remove` subcommand
  - [x] Deletes schedule by ID
  - [x] Shows confirmation message
