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

## Schedule UI Implementation (December 20, 2025)

### Tauri Commands
- [x] Added `add_schedule(project_root, workbook_path, cron_expression)` command
- [x] Added `list_schedules()` command
- [x] Added `update_schedule(schedule_id, cron_expression, enabled)` command
- [x] Added `delete_schedule(schedule_id)` command
- [x] Added `list_runs(limit)` command
- [x] Added scheduler manager initialization in AppState
- [x] Fixed Send trait issues in `SchedulerManager::update_schedule`

**Files:**
- `src-tauri/src/lib.rs` - Added Tauri commands and AppState updates
- `src-tauri/src/scheduler.rs` - Fixed Send trait issue by scoping database operations

### Frontend Components
- [x] Created `ScheduleTab.jsx` component with two sub-tabs:
  - Scheduled Workbooks tab with table view, add/edit/delete functionality
  - Recent Runs tab with run history display
- [x] Created `AddEditScheduleDialog` component for adding/editing schedules
  - Workbook selector for new schedules
  - Frequency presets (Daily, Hourly, Weekly, Custom)
  - Custom cron expression input
  - Enable/disable toggle (only shown when editing)
- [x] Updated `Sidebar.jsx` to open Schedule tab on click
- [x] Updated `App.jsx` to support 'schedule' tab type
- [x] Implemented schedule toggle (enable/disable)
- [x] Implemented schedule deletion with confirmation
- [x] Formatted timestamps, durations, and cron expressions
- [x] Added status badges for runs (success/failed/interrupted)
- [x] Added empty states for no schedules/runs

**Files:**
- `src/components/ScheduleTab.jsx` - New component
- `src/components/Sidebar.jsx` - Updated Schedule section
- `src/App.jsx` - Added ScheduleTab import and rendering

### Design Decisions
- New schedules are always created enabled (no checkbox in Add dialog)
- Enabled checkbox only shown when editing existing schedules
- Schedule tab opens as a regular tab, not a modal
- Follows style guide patterns (clean, minimal, grayscale + blue)
- Empty states with emojis for better UX

### Enhanced Scheduling UI (December 20, 2025)

**User-Friendly Scheduling Options:**
- [x] **Interval-based scheduling** - "Every X minutes/hours"
  - Number input (1-59 for minutes, 1-23 for hours)
  - Unit selector (minutes/hours)
  - Auto-validation when switching units
  - Preview text shows final schedule

- [x] **Daily with custom time** - "Daily at specific time"
  - Hour selector (00-23)
  - Minute selector (00-59)
  - Preview shows selected time in HH:MM format

- [x] **Weekly with day and time** - "Weekly on specific day"
  - Day of week selector (Sunday-Saturday)
  - Hour and minute selectors
  - Preview shows full schedule (e.g., "Monday at 9:15 AM")

- [x] **Smart cron formatting** - Displays human-readable schedules
  - "Every 5 minutes"
  - "Daily at 9:15 AM"
  - "Monday at 2:30 PM"
  - "Every 3 hours"
  - Falls back to "Custom: [cron]" for advanced patterns

**Technical Implementation:**
- Schedule type now defaults to "interval" (most common use case)
- Custom cron option moved to bottom as "advanced" option
- Cron expression builder handles all patterns correctly
- Parser detects existing schedules and populates UI fields
- 6-field cron format: `second minute hour day month weekday`

**Files Modified:**
- `src/components/ScheduleTab.jsx` - Enhanced scheduling UI

## Notes

**Schedule UI now fully functional with enhanced user-friendly scheduling!** Users can:
- Create schedules with **no technical knowledge required**:
  - "Every 5 minutes" - simple interval input
  - "Daily at 9:15 AM" - time picker with hour/minute selectors
  - "Monday at 2:30 PM" - day and time pickers
  - Advanced users can still use custom cron expressions
- View all scheduled workbooks in a table with human-readable frequency descriptions
- Edit existing schedules (automatically detects and populates UI fields)
- Enable/disable schedules with a single click
- Delete schedules with confirmation
- View recent run history with status, duration, and error messages

Still needed (future enhancements):
- Report file saving and viewing
- Run now button for manual execution
- Next run preview in schedule dialog
- Cron expression validation with user feedback
- Filter recent runs by status
- Show schedule count and next run in sidebar
