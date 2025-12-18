# Schedule - To Do

## Backend (Rust)

- [ ] Scheduler system
  - [ ] Cron parsing and evaluation
  - [ ] Background task runner (tokio tasks)
  - [ ] Schedule storage (SQLite)
  - [ ] Next run calculation
  - [ ] Scheduler lifecycle (start/stop/pause)

- [ ] Run tracking
  - [ ] Runs database (SQLite)
  - [ ] Run metadata storage
  - [ ] Report file management
  - [ ] Auto-cleanup old runs (keep last 30)

- [ ] Execution engine
  - [ ] Execute all cells in workbook sequentially
  - [ ] Capture outputs for each cell
  - [ ] Handle errors gracefully
  - [ ] Save notebook with outputs to .tether/runs/
  - [ ] Update run status (success/failed)

- [ ] Tauri commands
  - [ ] `add_schedule(workbook_path, cron, enabled)` - Create schedule
  - [ ] `list_schedules()` - Get all schedules
  - [ ] `update_schedule(id, cron, enabled)` - Update schedule
  - [ ] `delete_schedule(id)` - Delete schedule
  - [ ] `get_next_run(schedule_id)` - Calculate next run time
  - [ ] `list_runs(limit)` - Get recent runs
  - [ ] `get_run_report(run_id)` - Load run report notebook
  - [ ] `run_now(workbook_path)` - Manual execution (for testing)

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

- [ ] Cron expression parser
- [ ] Presets:
  - [ ] Daily at specific time
  - [ ] Hourly
  - [ ] Weekly on specific days
- [ ] Custom expression validator
- [ ] Next run calculator
- [ ] Human-readable description generator

## Run Cleanup

- [ ] Auto-delete runs older than 30th
- [ ] Clean up orphaned report files
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
