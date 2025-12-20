# Schedule - Completed

## Design & Planning

- [x] Feature specification complete
- [x] User flows defined
- [x] Database schema designed
- [x] Integration points identified

## Sidebar Placeholder

- [x] Schedule section placeholder with clock icon
- [x] Two-tab structure defined (Scheduled / Recent Runs)
- [x] Basic UI structure in place

## Backend Implementation (Dec 19, 2025)

### Engine Server - Execute All Cells

- [x] Added `/engine/execute-all` endpoint to `engine_server.py`
- [x] Execute cells sequentially with proper error handling
- [x] Collect outputs for each cell
- [x] Support for stopping on first error
- [x] Secret detection in outputs
- [x] Returns structured results with cell-level success/failure

**File:** `src-tauri/engine_server.py`

### Scheduler Module (Rust)

- [x] Created `scheduler.rs` module
- [x] Global SQLite database at `~/.tether/schedules.db`
- [x] Database schema for `schedules` and `runs` tables
- [x] Schedule CRUD operations (add, list, get, update, delete)
- [x] Run tracking (record, complete, list)
- [x] Cron expression validation via tokio-cron-scheduler
- [x] Cron presets (Daily, Hourly, Weekly)
- [x] Auto-cleanup for old runs

**File:** `src-tauri/src/scheduler.rs`

### CLI Binary

- [x] Created CLI entry point `src-tauri/src/cli.rs`
- [x] Added `clap` dependency for argument parsing
- [x] Added `tokio-cron-scheduler` for cron handling
- [x] Added `env_logger` for CLI logging
- [x] Updated `Cargo.toml` with binary definitions:
  - `tether` - CLI binary
  - `tether-gui` - GUI binary

**Commands Implemented:**

- `tether run <notebook>` - **FULLY IMPLEMENTED** (Dec 19, 2024)
  - Executes all cells in a notebook via engine server
  - Auto-detects project by walking up to find `.tether` directory
  - Ensures Python venv and syncs dependencies
  - Displays execution results and outputs in terminal
  - Shows summary with cell counts and success/failure status
- `tether schedule add <notebook> --cron <expr>` - Add schedule with custom cron
- `tether schedule add <notebook> --daily` - Add daily schedule (9am)
- `tether schedule add <notebook> --hourly` - Add hourly schedule
- `tether schedule add <notebook> --weekly` - Add weekly schedule (Mon 9am)
- `tether schedule list` - List all schedules
- `tether schedule remove <id>` - Remove a schedule

**Files:**
- `src-tauri/src/cli.rs`
- `src-tauri/Cargo.toml`
- `src-tauri/src/lib.rs` (made modules public)
- `src-tauri/src/engine_http.rs` (added `execute_all_http`)

### Documentation

- [x] Created `implementation-notes.md` with exploration findings
- [x] Documented current state, architecture decisions, and next steps

**File:** `features/schedule/implementation-notes.md`

### Background Scheduler Task Runner (December 19, 2024)

- [x] Implemented background scheduler using tokio-cron-scheduler
- [x] Added `execute_scheduled_workbook()` - Executes a workbook when schedule triggers
- [x] Added `execute_workbook_internal()` - Core workbook execution logic
  - Parses notebook to extract cells
  - Ensures Python environment exists
  - Starts engine server
  - Executes all cells via `/engine/execute-all`
  - Records run status (success/failed)
  - Cleans up old runs (keeps last 30)
- [x] Added `load_all_schedules()` - Loads all enabled schedules on startup
- [x] Added `register_schedule_job()` - Registers a schedule as a cron job
- [x] Added `unregister_schedule_job()` - Removes a schedule's job
- [x] Updated `start_scheduler()` - Now loads and registers all schedules
- [x] Made `add_schedule()` async - Automatically registers job when created
- [x] Made `update_schedule()` async - Re-registers job when modified
- [x] Made `delete_schedule()` async - Unregisters job when deleted
- [x] Added job_map to track schedule_id → job_id mapping
- [x] Updated CLI to use new async methods

**Architecture:**
- When app starts, `start_scheduler()` creates a JobScheduler and loads all enabled schedules
- Each schedule is registered as a cron job with tokio-cron-scheduler
- When a job triggers, `execute_scheduled_workbook()` runs:
  1. Records run start in database
  2. Executes workbook via engine server
  3. Saves execution status and outputs
  4. Updates schedule's last_run timestamp
  5. Cleans up old runs
- Jobs are dynamically added/removed when schedules are created/deleted/updated

**Files:**
- `src-tauri/src/scheduler.rs` (major updates)
- `src-tauri/src/cli.rs` (updated to use async methods)

## Notes

**Background scheduler now fully implemented!** Schedules automatically execute when the app is running. Still needed:
- Tauri commands for GUI integration
- Frontend UI components (see `todo.md`)
- Report file saving (currently stored in database only)
