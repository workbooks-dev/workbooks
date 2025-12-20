# Schedule - To Do

## 🎯 Current Status (Dec 19, 2025)

**✅ Backend Complete:**
- Scheduler module with SQLite database (`~/.tether/schedules.db`)
- CLI commands for managing schedules
- Engine server endpoint for executing all cells
- Run tracking and cleanup functions
- **Background scheduler task runner** ✅ **(Completed Dec 19, 2025)**
  - Automatically executes workbooks on schedule
  - Dynamic job registration when schedules change
  - Full integration with engine server

**🚧 Next Steps:**
- Add Tauri commands for GUI integration
- Build frontend Schedule tab UI
- Implement report file saving

**📝 See `features/schedule/done.md` for detailed implementation notes.**

---

## Backend (Rust)

- [x] Scheduler system ✅ **(Completed Dec 19, 2025)**
  - [x] Cron parsing and evaluation (via tokio-cron-scheduler)
  - [x] Background task runner (tokio tasks) ✅ **(Completed Dec 19, 2025)**
  - [x] Schedule storage (SQLite) - Global database at `~/.tether/schedules.db`
  - [x] Next run calculation (framework in place, needs full implementation)
  - [x] Scheduler lifecycle (start/stop/pause)

- [x] Run tracking ✅ **(Completed Dec 19, 2025)**
  - [x] Runs database (SQLite)
  - [x] Run metadata storage
  - [ ] Report file management - **NEEDS IMPLEMENTATION**
  - [x] Auto-cleanup old runs (keep last 30)

- [x] Execution engine ✅ **(Completed Dec 19, 2025)**
  - [x] Execute all cells in workbook sequentially - `/engine/execute-all` endpoint
  - [x] Capture outputs for each cell
  - [x] Handle errors gracefully - Stops on first error
  - [ ] Save notebook with outputs to .tether/runs/ - **NEEDS IMPLEMENTATION** (reports in DB only)
  - [x] Update run status (success/failed) - `complete_run()` function exists
  - [x] Scheduled execution fully working ✅ **(Completed Dec 19, 2025)**

- [ ] Tauri commands - **NEEDS IMPLEMENTATION**
  - [ ] `add_schedule(workbook_path, cron, enabled)` - Create schedule
  - [ ] `list_schedules()` - Get all schedules
  - [ ] `update_schedule(id, cron, enabled)` - Update schedule
  - [ ] `delete_schedule(id)` - Delete schedule
  - [ ] `get_next_run(schedule_id)` - Calculate next run time
  - [ ] `list_runs(limit)` - Get recent runs
  - [ ] `get_run_report(run_id)` - Load run report notebook
  - [ ] `run_now(workbook_path)` - Manual execution (for testing)

## CLI Commands ✅ **(Completed Dec 19, 2025)**

- [x] `tether run <notebook>` - Run workbook (placeholder, needs engine integration)
- [x] `tether schedule add <notebook> --cron <expr>` - Add custom cron schedule
- [x] `tether schedule add <notebook> --daily` - Daily at 9am preset
- [x] `tether schedule add <notebook> --hourly` - Hourly preset
- [x] `tether schedule add <notebook> --weekly` - Weekly on Monday preset
- [x] `tether schedule list` - List all schedules
- [x] `tether schedule remove <id>` - Remove a schedule

**Files:** `src-tauri/src/cli.rs`, `src-tauri/Cargo.toml`

## Frontend (React)

- [ ] Schedule tab component
  - [ ] Two-tab layout (Scheduled Workbooks / Recent Runs)
  - [ ] Open as tab (not modal)

- [ ] Scheduled Workbooks tab
  - [ ] Table view of active schedules
  - [ ] Columns: Name, Frequency, Next Run, Toggle, Actions
  - [ ] "+ Add Schedule" button and dialog
  - [ ] Edit schedule dialog
  - [ ] Delete confirmation dialog
  - [ ] Enable/disable toggle

- [ ] Add/Edit Schedule dialog
  - [ ] Workbook selector
  - [ ] Frequency presets (Daily, Hourly, Weekly)
  - [ ] Custom cron expression input
  - [ ] Next run preview
  - [ ] Cron validation
  - [ ] Save/cancel buttons

- [ ] Recent Runs tab
  - [ ] Table view of last 30 runs
  - [ ] Columns: Workbook, Started At, Duration, Status, Actions
  - [ ] "View Report" button
  - [ ] Status indicators (success/failed icons)
  - [ ] Filters (all/success/failed)

- [ ] Run report viewer
  - [ ] Open report in read-only tab
  - [ ] Display saved notebook with outputs
  - [ ] "Report" indicator in tab
  - [ ] Cannot edit cells
  - [ ] Can copy code

## Sidebar Integration

- [ ] Schedule section enhancements
  - [ ] Show scheduled workbook count
  - [ ] Show next upcoming run time
  - [ ] Click header → Open schedule tab
  - [ ] Visual indicator for active schedules

## Workbooks Table Integration

- [ ] "Schedule" button in Actions column
  - [ ] Opens Add Schedule dialog
  - [ ] Pre-fills workbook name
  - [ ] Quick access to automation

- [ ] Scheduled indicator
  - [ ] Show if workbook is scheduled
  - [ ] Display frequency/next run

## Cron Handling

- [x] Cron expression parser ✅ (via tokio-cron-scheduler)
- [x] Presets: ✅ **(Completed Dec 19, 2025)**
  - [x] Daily at specific time (9am)
  - [x] Hourly
  - [x] Weekly on specific days (Monday 9am)
- [x] Custom expression validator ✅
- [ ] Next run calculator - **NEEDS FULL IMPLEMENTATION** (framework exists)
- [ ] Human-readable description generator - **NEEDS IMPLEMENTATION**

## Run Cleanup

- [x] Auto-delete runs older than 30th ✅ **(Completed Dec 19, 2025)** - `cleanup_old_runs()` function
- [ ] Clean up orphaned report files - **NEEDS IMPLEMENTATION**
- [ ] Configurable retention (future)

## Notifications (Future)

- [ ] Desktop notifications on run completion
- [ ] Success/failure status
- [ ] Click notification → View report
- [ ] Email notifications (optional)

## System Integration (Future)

- [ ] Run scheduler as system service/daemon
- [ ] Execute schedules even when app closed
- [ ] Platform-specific implementations:
  - [ ] macOS: launchd
  - [ ] Windows: Task Scheduler
  - [ ] Linux: systemd

## Testing

- [ ] Test cron parsing and next run calculation
- [ ] Test scheduled execution
- [ ] Test run report generation
- [ ] Test cleanup of old runs
- [ ] Test pause/resume on app close/open
