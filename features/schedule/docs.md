# Schedule & Run History

## Overview

Tether allows workbooks to run on automated schedules (cron-style) and tracks execution history for auditing and debugging.

## Design Philosophy

**Automated Data Pipelines:**
- Set it and forget it
- Run workbooks daily, hourly, weekly
- Reliable execution while app is running
- No external dependencies (no cloud services)

**Audit Trail:**
- Keep last 30 runs
- View exactly what happened
- Debug failures easily
- Track performance over time

## User Experience

### Sidebar Section

**Schedule Overview:**
- Clock icon header
- Shows count of scheduled workbooks
- Shows next upcoming run
- Click header → Opens schedule tab

### Schedule Tab

**Two-Tab Interface:**

#### Tab 1: Scheduled Workbooks

**List View:**
- All workbooks with active schedules
- Columns:
  - Name (workbook filename)
  - Frequency (Daily at 9am, Every hour, Custom cron)
  - Next Run (timestamp)
  - Toggle (enable/disable)
  - Actions (Edit, Delete)

**Add Schedule:**
- "+ Add Schedule" button
- Select workbook
- Choose frequency:
  - Daily (select time)
  - Hourly
  - Weekly (select days and time)
  - Custom (cron expression)
- Save

**Edit Schedule:**
- Click Edit button
- Modify frequency/time
- Update

**Delete Schedule:**
- Click Delete button
- Confirmation dialog
- Remove schedule

#### Tab 2: Recent Runs

**List View:**
- Last 30 runs across all workbooks
- Columns:
  - Workbook Name
  - Started At (timestamp)
  - Duration (seconds)
  - Status (Success, Failed, Interrupted)
  - Actions (View Report)

**Auto-Deletion:**
- Keeps only last 30 runs
- Oldest automatically deleted
- Per-workbook and global limits

**Run Reports:**
- Click "View Report" to see saved notebook output
- Shows exactly what happened during run
- Outputs, errors, full execution trace
- Opens in read-only mode

### From Workbook Table

**Quick Schedule:**
- "Schedule" button in Workbooks table view
- Opens schedule dialog pre-filled with workbook name
- One-click to automate

## Technical Implementation

### Scheduler Backend (Rust)

**Architecture:**
- Cron-based scheduling using Rust crate
- Background task runner
- Runs while Tauri app is open
- Pauses when app closes

**Data Storage:**
- `.tether/schedules.db` - SQLite database for schedules
- `.tether/runs/` - Directory for run reports

**Schema:**
```sql
CREATE TABLE schedules (
  id TEXT PRIMARY KEY,
  workbook_path TEXT NOT NULL,
  cron_expression TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL,
  modified_at INTEGER NOT NULL,
  next_run INTEGER
);

CREATE TABLE runs (
  id TEXT PRIMARY KEY,
  workbook_path TEXT NOT NULL,
  schedule_id TEXT,  -- null for manual runs
  started_at INTEGER NOT NULL,
  finished_at INTEGER,
  duration INTEGER,
  status TEXT NOT NULL,  -- success, failed, interrupted
  error_message TEXT,
  report_path TEXT,
  FOREIGN KEY (schedule_id) REFERENCES schedules(id)
);
```

### Execution Flow

**Scheduled Run:**
1. Scheduler checks for due runs every minute
2. Finds workbook with `next_run <= now()`
3. Starts engine for workbook
4. Executes all cells sequentially
5. Captures outputs and errors
6. Saves notebook with outputs to `.tether/runs/{run_id}.ipynb`
7. Records run metadata in database
8. Updates `next_run` based on cron expression
9. Cleans up old runs (keep last 30)

**Run Report:**
- Copy of notebook with execution outputs
- Timestamped filename: `{workbook_name}_{timestamp}.ipynb`
- Stored in `.tether/runs/`
- Can be opened in Tether or any Jupyter viewer

### Cron Expressions

**Simple Presets:**
- Daily at 9am: `0 9 * * *`
- Every hour: `0 * * * *`
- Weekly on Monday at 9am: `0 9 * * 1`

**Custom:**
- Full cron syntax support
- Validation on save
- User-friendly error messages

**UI Helpers:**
- Preset picker (Daily, Hourly, Weekly)
- Custom cron builder
- Next run preview

### Background Execution

**App Running:**
- Scheduler active when app open
- Runs execute in background
- Notifications on completion (optional)
- Does not block UI

**App Closed:**
- With system tray: Scheduler continues running in background
- Without system tray (old behavior): Scheduler pauses, missed runs shown on next start

## System Tray Background Process

**Overview:**
Tether runs as a menu bar/system tray application (like Docker Desktop, Ollama) to enable reliable scheduling even when the main window is closed.

**Architecture Decision:**
- **System Tray App** (recommended approach)
  - Main window can close, background process continues
  - Menu bar icon provides quick access and status
  - Native Tauri `SystemTray` API support
  - Familiar UX pattern (Docker, Ollama, Postgres.app)
  - Clean mental model: quit = stop schedules, hide = keep running

**Alternative Approaches Considered:**
- Separate daemon process - More complex, requires IPC/HTTP between processes
- System service (launchd/systemd) - More setup complexity, harder installation
- System tray chosen for simplicity and user familiarity

### System Tray Features

**Menu Bar Icon:**
- Shows Tether status at a glance
- Always visible when scheduler is active
- Click to reveal menu

**Menu Options:**
- **Open Tether** - Shows main window
- **Scheduler Status** - Running / Paused / X schedules active
- **Next Run** - Shows upcoming scheduled workbook and time
- **Separator**
- **Pause Scheduler** - Temporarily disable all schedules
- **Resume Scheduler** - Re-enable schedules
- **Separator**
- **Quit Tether** - Stops background process entirely

**Status Indicators:**
- Idle: Gray icon
- Running workbook: Blue animated icon
- Error: Red icon with badge
- Paused: Gray icon with pause symbol

**Window Behavior:**
- Closing window (X button) → Hides window, keeps app running
- Main window can be re-opened from tray
- Quit from menu → Stops scheduler and exits app completely

### Implementation Details

**Tauri SystemTray:**
```rust
use tauri::{CustomMenuItem, SystemTray, SystemTrayMenu, SystemTrayEvent};

let tray_menu = SystemTrayMenu::new()
    .add_item(CustomMenuItem::new("open", "Open Tether"))
    .add_item(CustomMenuItem::new("status", "Scheduler: Running"))
    .add_native_item(SystemTrayMenuItem::Separator)
    .add_item(CustomMenuItem::new("pause", "Pause Scheduler"))
    .add_item(CustomMenuItem::new("quit", "Quit Tether"));

let system_tray = SystemTray::new().with_menu(tray_menu);
```

**Window Close Prevention:**
```rust
.on_window_event(|event| match event.event() {
    WindowEvent::CloseRequested { api, .. } => {
        // Hide window instead of closing app
        event.window().hide().unwrap();
        api.prevent_close();
    }
    _ => {}
})
```

**Dynamic Menu Updates:**
- Update status text when schedules run
- Show countdown to next run
- Update icon based on scheduler state
- Refresh menu on schedule changes

### User Experience

**First Launch:**
- App opens with main window
- Menu bar icon appears
- User can close window, app stays running

**Subsequent Use:**
- Click menu bar icon → Open Tether
- Schedules run in background
- Notifications on completion (optional)

**Quitting:**
- Menu → Quit Tether
- Icon disappears from menu bar
- Scheduler stops, no more scheduled runs
- Clean shutdown

**Platform-Specific:**
- macOS: Menu bar (top right)
- Windows: System tray (bottom right)
- Linux: System tray (varies by DE)

### Benefits

1. **Reliable Scheduling** - Runs continue even when window closed
2. **Quick Access** - Always one click away
3. **Status Visibility** - See scheduler state at a glance
4. **Familiar Pattern** - Users understand menu bar apps
5. **Clean Exit** - Quit explicitly stops everything

## Run History

### Storage

**Run Reports:**
- `.tether/runs/{run_id}.ipynb` - Full notebook with outputs
- Metadata in `runs` table
- Auto-cleanup after 30 runs per workbook

**Retention:**
- Last 30 runs globally
- Configurable in settings (future)
- Manual runs and scheduled runs both tracked

### Viewing Reports

**From Recent Runs Tab:**
- Click "View Report" button
- Opens saved notebook in read-only mode
- Shows outputs exactly as they were
- Includes error messages and tracebacks

**Read-Only Mode:**
- Cannot edit cells
- Can view all outputs
- Can copy code
- Clear "Report" indicator in tab

## Integration Points

### With Workbooks

**Manual Runs:**
- Also tracked in run history
- Useful for comparing manual vs scheduled
- Same report format

**Schedule Button:**
- In Workbooks table view
- Quick access to scheduling
- Shows if already scheduled

### With Sidebar

**Quick Overview:**
- Scheduled count
- Next run time
- Click to see details

### With Notifications (Future)

**Run Completion:**
- Desktop notification
- Success/failure status
- Click to view report

**Failures:**
- Highlight in Recent Runs
- Email notification (optional)
- Retry options

## Use Cases

**Daily Data Sync:**
- Schedule "Fetch Stripe Orders" to run at 9am daily
- Automatically pulls latest data
- Ready for analysis when you start work

**Hourly Monitoring:**
- Schedule "Check API Health" every hour
- Track uptime and performance
- Alert on failures

**Weekly Reports:**
- Schedule "Generate Sales Report" every Monday
- Automate routine tasks
- Consistent execution

**One-Off Automation:**
- Schedule workbook to run once at specific time
- Delete schedule after execution
- Useful for future-dated tasks
