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
- Scheduler pauses
- Missed runs shown on next start
- Option to run missed schedules immediately

**Future:**
- System service/daemon for always-on scheduling
- Run even when app is closed
- Platform-specific implementations

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
