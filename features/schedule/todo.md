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

- [x] Tauri commands ✅ **(Completed Dec 20, 2025)**
  - [x] `add_schedule(workbook_path, cron)` - Create schedule (always enabled by default)
  - [x] `list_schedules()` - Get all schedules
  - [x] `update_schedule(id, cron, enabled)` - Update schedule
  - [x] `delete_schedule(id)` - Delete schedule
  - [x] `list_runs(limit)` - Get recent runs
  - [ ] `get_run_report(run_id)` - Load run report notebook - **FUTURE**
  - [ ] `run_now(workbook_path)` - Manual execution (for testing) - **FUTURE**

## CLI Commands ✅ **(Completed Dec 19, 2025)**

- [x] `tether run <notebook>` - Run workbook (placeholder, needs engine integration)
- [x] `tether schedule add <notebook> --cron <expr>` - Add custom cron schedule
- [x] `tether schedule add <notebook> --daily` - Daily at 9am preset
- [x] `tether schedule add <notebook> --hourly` - Hourly preset
- [x] `tether schedule add <notebook> --weekly` - Weekly on Monday preset
- [x] `tether schedule list` - List all schedules
- [x] `tether schedule remove <id>` - Remove a schedule

**Files:** `src-tauri/src/cli.rs`, `src-tauri/Cargo.toml`

## Frontend (React) ✅ **(Completed Dec 20, 2025)**

- [x] Schedule tab component ✅
  - [x] Two-tab layout (Scheduled Workbooks / Recent Runs)
  - [x] Opens as tab (not modal)

- [x] Scheduled Workbooks tab ✅
  - [x] Table view of active schedules
  - [x] Columns: Name, Frequency, Next Run, Last Run, Status (enabled/disabled), Actions
  - [x] "+ Add Schedule" button and dialog
  - [x] Edit schedule dialog
  - [x] Delete confirmation dialog
  - [x] Enable/disable toggle

- [x] Add/Edit Schedule dialog ✅
  - [x] Workbook selector (for new schedules)
  - [x] Frequency presets (Daily, Hourly, Weekly, Custom)
  - [x] Custom cron expression input
  - [x] Save/cancel buttons
  - [ ] Next run preview - **FUTURE ENHANCEMENT**
  - [ ] Cron validation feedback - **FUTURE ENHANCEMENT**

- [x] Recent Runs tab ✅
  - [x] Table view of last 30 runs
  - [x] Columns: Workbook, Started At, Duration, Status, Error Message
  - [x] Status badges (success/failed/interrupted)
  - [ ] "View Report" button - **FUTURE** (requires report file saving)
  - [ ] Filters (all/success/failed) - **FUTURE ENHANCEMENT**

- [ ] Run report viewer - **FUTURE**
  - [ ] Open report in read-only tab
  - [ ] Display saved notebook with outputs
  - [ ] "Report" indicator in tab
  - [ ] Cannot edit cells
  - [ ] Can copy code

## Sidebar Integration ✅ **(Completed Dec 20, 2025)**

- [x] Schedule section enhancements ✅
  - [x] Click header → Open schedule tab
  - [x] "Manage Schedule" button
  - [ ] Show scheduled workbook count - **FUTURE ENHANCEMENT**
  - [ ] Show next upcoming run time - **FUTURE ENHANCEMENT**
  - [ ] Visual indicator for active schedules - **FUTURE ENHANCEMENT**

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

## System Tray Background Process ✅ **(Core Implemented Dec 20, 2024)**

**Status:** Basic system tray implemented. Advanced features (dynamic updates, pause/resume, icons) are future enhancements.

### Backend (Rust)

- [x] System tray setup ✅ **(Dec 20, 2024)**
  - [x] Add `tauri` system tray dependencies (tray-icon feature)
  - [x] Create system tray menu structure
  - [x] Initialize SystemTray in lib.rs
  - [ ] Design menu bar icons (idle, running, error, paused states) - **FUTURE ENHANCEMENT**

- [x] Basic menu items and actions ✅ **(Dec 20, 2024)**
  - [x] "Open Tether" - Show main window
  - [x] "Scheduler: Running" - Static status display (not clickable)
  - [x] "Quit Tether" - Full app shutdown
  - [ ] "Next Run: [Time]" - Shows upcoming schedule (not clickable) - **FUTURE ENHANCEMENT**
  - [ ] "Pause Scheduler" / "Resume Scheduler" - Toggle scheduler state - **FUTURE ENHANCEMENT**

- [x] Window behavior ✅ **(Dec 20, 2024)**
  - [x] Intercept window close event
  - [x] Hide window instead of closing app
  - [x] Re-show window on "Open Tether" click
  - [x] Proper shutdown on "Quit" from menu

- [ ] Dynamic menu updates
  - [ ] Update status text when schedules run
  - [ ] Update next run time display
  - [ ] Change icon based on scheduler state
  - [ ] Refresh menu when schedules change

- [ ] Event handlers
  - [ ] System tray click events
  - [ ] Menu item click routing
  - [ ] Update tray on scheduler events (start run, complete run, error)

### Frontend Integration

- [ ] Tauri commands for tray
  - [ ] `update_tray_status(status)` - Update status in menu
  - [ ] `update_next_run(time)` - Update next run display
  - [ ] `set_tray_icon(state)` - Change icon (idle/running/error/paused)

- [ ] React hooks
  - [ ] Hook to update tray when schedules change
  - [ ] Hook to update tray during workbook execution
  - [ ] Hook to show window on tray "Open" click

### Icon Design

- [ ] Create icon set
  - [ ] Idle state (gray)
  - [ ] Running state (blue, animated if possible)
  - [ ] Error state (red with badge)
  - [ ] Paused state (gray with pause symbol)
  - [ ] Platform-specific sizes (macOS: 16x16@2x, Windows: 16x16/32x32)

### Testing

- [ ] Test window hide/show behavior
- [ ] Test scheduler continues when window closed
- [ ] Test quit fully stops scheduler
- [ ] Test menu updates reflect actual state
- [ ] Test on all platforms (macOS, Windows, Linux)

## Notifications (Future)

- [ ] Desktop notifications on run completion
- [ ] Success/failure status
- [ ] Click notification → View report
- [ ] Email notifications (optional)
- [ ] Integration with system tray - click notification shows relevant tray menu item

## Advanced System Integration (Future)

- [ ] Optional system service/daemon mode (for advanced users)
- [ ] Run as true background service (beyond system tray)
- [ ] Platform-specific implementations:
  - [ ] macOS: launchd plist
  - [ ] Windows: Windows Service
  - [ ] Linux: systemd unit
- [ ] Auto-start on login option
- [ ] Service management UI in settings

## Testing

- [ ] Test cron parsing and next run calculation
- [ ] Test scheduled execution
- [ ] Test run report generation
- [ ] Test cleanup of old runs
- [ ] Test pause/resume on app close/open
