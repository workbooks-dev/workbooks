# Schedule - To Do

## High Priority

- [ ] **Pending event display with cancellation**
  - [ ] Show next scheduled run in UI (1 upcoming event only)
  - [ ] "Cancel Next Run" button to skip the upcoming execution
  - [ ] Re-calculates next run after cancellation
- [ ] filename changes? We need a way to "reactivate" a scheduled notebook

## Medium Priority

### Backend

- [ ] Report file management
  - [ ] Save executed notebook with outputs to `.tether/runs/{run_id}.ipynb`
  - [ ] `get_run_report(run_id)` Tauri command to load report
  - [ ] Clean up orphaned report files on cleanup

- [ ] Manual execution command
  - [ ] `run_now(workbook_path)` Tauri command for manual runs (for testing)

- [ ] Next run calculator - Full implementation (framework exists)
- [ ] Human-readable cron description generator

### Frontend

- [ ] Run report viewer
  - [ ] Open report in read-only tab
  - [ ] Display saved notebook with outputs
  - [ ] "Report" indicator in tab
  - [ ] Cannot edit cells
  - [ ] Can copy code

- [ ] Schedule dialog enhancements
  - [ ] Next run preview
  - [ ] Cron validation feedback with user-friendly errors

- [ ] Recent Runs tab enhancements
  - [ ] "View Report" button (requires report file saving)
  - [ ] Filters (all/success/failed/interrupted)

- [ ] Sidebar enhancements
  - [ ] Show scheduled workbook count
  - [ ] Show next upcoming run time
  - [ ] Visual indicator for active schedules

### Workbooks Table Integration

- [ ] "Schedule" button in Actions column
  - [ ] Opens Add Schedule dialog
  - [ ] Pre-fills workbook name
  - [ ] Quick access to automation

- [ ] Scheduled indicator
  - [ ] Show if workbook is scheduled
  - [ ] Display frequency/next run

## Low Priority

### System Tray Enhancements

- [ ] Design menu bar icons (idle, running, error, paused states)
- [ ] Dynamic menu updates
  - [ ] Update status text when schedules run
  - [ ] Update next run time display
  - [ ] Change icon based on scheduler state
  - [ ] Refresh menu when schedules change

- [ ] Event handlers
  - [ ] System tray click events
  - [ ] Menu item click routing
  - [ ] Update tray on scheduler events (start run, complete run, error)

- [ ] Pause/Resume functionality
  - [ ] "Pause Scheduler" / "Resume Scheduler" menu items
  - [ ] "Next Run: [Time]" shows upcoming schedule

- [ ] Tauri commands for tray
  - [ ] `update_tray_status(status)` - Update status in menu
  - [ ] `update_next_run(time)` - Update next run display
  - [ ] `set_tray_icon(state)` - Change icon (idle/running/error/paused)

- [ ] React hooks
  - [ ] Hook to update tray when schedules change
  - [ ] Hook to update tray during workbook execution
  - [ ] Hook to show window on tray "Open" click

- [ ] Icon design
  - [ ] Idle state (gray)
  - [ ] Running state (blue, animated if possible)
  - [ ] Error state (red with badge)
  - [ ] Paused state (gray with pause symbol)
  - [ ] Platform-specific sizes (macOS: 16x16@2x, Windows: 16x16/32x32)

### Notifications

- [ ] Desktop notifications on run completion
- [ ] Success/failure status
- [ ] Click notification → View report
- [ ] Email notifications (optional)
- [ ] Integration with system tray - click notification shows relevant tray menu item

### Advanced Features

- [ ] Configurable retention period for run history
- [ ] Optional system service/daemon mode (for advanced users)
- [ ] Platform-specific implementations:
  - [ ] macOS: launchd plist
  - [ ] Windows: Windows Service
  - [ ] Linux: systemd unit
- [ ] Auto-start on login option
- [ ] Service management UI in settings

## Testing

- [ ] Test cron parsing and next run calculation
- [ ] Test scheduled execution across timezones
- [ ] Test run report generation
- [ ] Test cleanup of old runs
- [ ] Test pause/resume on app close/open
- [ ] Test window hide/show behavior
- [ ] Test scheduler continues when window closed
- [ ] Test quit fully stops scheduler
- [ ] Test menu updates reflect actual state
- [ ] Test on all platforms (macOS, Windows, Linux)
