# Notifications

Unified notification system for background events, errors, run completions, and updates.

## Overview

Workbooks runs in the background via system tray. When important events happen (scheduled runs complete, errors occur, updates available), users need to be informed without being intrusive.

## Notification Types

### 1. **Run Completions**
- Scheduled workbook finished executing
- Manual run completed in background
- Show: workbook name, success/failure, duration
- Action: Click to view run details or open workbook

### 2. **Errors**
- Workbook execution failed
- Engine crashed or disconnected
- Package installation failed
- Show: error type, workbook/context, brief message
- Action: Click to view full error details or logs

### 3. **Updates Available**
- New version of Workbooks available
- Show: current version → new version, changelog highlights
- Action: Click to view changelog and install update

### 4. **Secrets/Security** (Future)
- Secrets session expired, re-authentication needed
- Suspicious activity detected

## UI Locations

### System Notifications (Native)
- macOS: System notification center
- Critical events only (errors, completed long-running tasks)
- Respects system Do Not Disturb settings
- Tauri plugin: `tauri-plugin-notification`

### In-App Notification Center
- **Tray Menu Section**: "Recent Notifications" submenu showing last 3-5
- **In-App Panel**: Click notification icon in toolbar to expand list
- Shows all notifications from last 7 days
- Grouped by type with icons and colors
- Mark as read/unread
- Clear all / clear by type

### Tray Badge (macOS)
- Show count of unread notifications
- Clear when user opens notification center

## Data Model

```rust
pub struct Notification {
    pub id: String,              // UUID
    pub notification_type: NotificationType,
    pub title: String,
    pub message: String,
    pub timestamp: i64,          // Unix timestamp
    pub read: bool,
    pub dismissed: bool,

    // Context for actions
    pub related_run_id: Option<String>,
    pub related_workbook: Option<String>,
    pub related_project: Option<String>,

    // Type-specific metadata (JSON)
    pub metadata: Option<String>,
}

pub enum NotificationType {
    RunSuccess,
    RunFailure,
    Error,
    UpdateAvailable,
    Warning,
    Info,
}
```

## Storage

**SQLite database**: `~/.workbooks/notifications.db`
- Global across all projects
- Stores last 30 days, auto-prune older
- Indexes on: timestamp, type, read status

```sql
CREATE TABLE notifications (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    title TEXT NOT NULL,
    message TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    read INTEGER DEFAULT 0,
    dismissed INTEGER DEFAULT 0,
    related_run_id TEXT,
    related_workbook TEXT,
    related_project TEXT,
    metadata TEXT,
    created_at INTEGER DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX idx_timestamp ON notifications(timestamp DESC);
CREATE INDEX idx_type ON notifications(type);
CREATE INDEX idx_read ON notifications(read);
```

## Notification Manager (Rust)

**Location**: `src-tauri/src/notifications.rs`

```rust
pub struct NotificationManager {
    db_path: PathBuf,
    conn: rusqlite::Connection,
}

impl NotificationManager {
    pub fn new() -> Result<Self>;

    // Create notifications
    pub fn notify_run_success(&self, run_id: &str, workbook: &str, duration: f64) -> Result<Notification>;
    pub fn notify_run_failure(&self, run_id: &str, workbook: &str, error: &str) -> Result<Notification>;
    pub fn notify_error(&self, title: &str, message: &str, context: Option<String>) -> Result<Notification>;
    pub fn notify_update_available(&self, current: &str, new: &str, changelog: &str) -> Result<Notification>;

    // Query notifications
    pub fn list_notifications(&self, limit: usize, offset: usize) -> Result<Vec<Notification>>;
    pub fn get_unread_count(&self) -> Result<usize>;
    pub fn get_recent(&self, limit: usize) -> Result<Vec<Notification>>;

    // Update notifications
    pub fn mark_read(&self, id: &str) -> Result<()>;
    pub fn mark_all_read(&self) -> Result<()>;
    pub fn dismiss(&self, id: &str) -> Result<()>;
    pub fn clear_all(&self) -> Result<()>;

    // Cleanup
    pub fn prune_old(&self, days: i64) -> Result<usize>; // Delete older than N days
}
```

## Tauri Commands

```rust
#[tauri::command]
async fn list_notifications(limit: usize, offset: usize, state: State<AppState>) -> Result<Vec<Notification>, String>;

#[tauri::command]
async fn get_unread_count(state: State<AppState>) -> Result<usize, String>;

#[tauri::command]
async fn mark_notification_read(id: String, state: State<AppState>) -> Result<(), String>;

#[tauri::command]
async fn mark_all_notifications_read(state: State<AppState>) -> Result<(), String>;

#[tauri::command]
async fn dismiss_notification(id: String, state: State<AppState>) -> Result<(), String>;

#[tauri::command]
async fn clear_all_notifications(state: State<AppState>) -> Result<(), String>;
```

## Frontend Components

### NotificationCenter.jsx
- Slide-out panel from right side or dropdown from toolbar
- List of notifications with icons, timestamps, read status
- Click notification → navigate to relevant context (run details, workbook, settings)
- "Mark all read" button
- Filter by type

### NotificationBadge.jsx
- Bell icon in toolbar with badge count
- Red dot or number showing unread count
- Click to toggle NotificationCenter

### TrayNotificationItem
- Show in tray menu: "🔔 3 new notifications"
- Submenu with recent 3-5 notifications
- Click notification → open app and navigate to context
- "View All" → opens app to notification center

## Native System Notifications

Use `tauri-plugin-notification` for native OS notifications:
- Only for important events (errors, completed runs when app backgrounded)
- Don't spam - use rate limiting
- User preference to enable/disable in settings

```rust
use tauri_plugin_notification::NotificationExt;

// Send native notification
app.notification()
    .builder()
    .title("Run completed")
    .body("data_pipeline.ipynb finished successfully")
    .show()?;
```

## Settings/Preferences

**User controls** (in Settings UI):
- Enable/disable system notifications
- Enable/disable notification sounds
- Choose which event types trigger notifications
- Notification retention (7/14/30 days)

## Event Flow

### Run Completion Example:
1. Scheduler executes workbook → run completes
2. Scheduler calls `NotificationManager::notify_run_success()`
3. NotificationManager:
   - Inserts notification into DB
   - Emits Tauri event: `notification:new`
   - If app is backgrounded and setting enabled: send OS notification
4. Frontend receives event → updates notification badge count
5. User clicks notification → navigates to run details

## Integration Points

### Scheduler
When run completes/fails:
```rust
let notification_manager = state.notification_manager.lock().await;
notification_manager.notify_run_success(run_id, workbook_path, duration)?;
```

### Engine Execution
When cell execution fails:
```rust
notification_manager.notify_error(
    "Cell execution failed",
    &error_message,
    Some(workbook_path)
)?;
```

### Update Checker
When update detected:
```rust
notification_manager.notify_update_available(
    current_version,
    latest_version,
    &changelog
)?;
```

## Future Enhancements

- Rich notifications with inline actions (macOS)
- Notification grouping (e.g., "5 runs completed")
- Email/webhook notifications for critical events
- Per-project notification preferences
- Notification history export
- Desktop notification position preference (OS permitting)
