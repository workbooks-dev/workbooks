# Tray Menu

## Overview

The system tray icon keeps Tether running in the background even when all windows are closed. This is essential for the scheduler to continue running automated workbook executions.

## Design Goals

1. **Always Available** - App accessible from menu bar even with no open windows
2. **Scheduler Continuity** - Background tasks continue running when windows are closed
3. **Quick Access** - Fast access to recent projects and common actions
4. **Status Visibility** - Show scheduler status at a glance

## Menu Structure

```
Tether (icon)
├── Recent Projects (dynamic, max 3)
│   ├── Project Name 1
│   ├── Project Name 2
│   └── Project Name 3
├── ─────────────────
├── Create Project...
├── Open Project...
├── ─────────────────
├── View Runs
├── View Scheduler
├── ─────────────────
├── Install MCP...
├── ─────────────────
├── Scheduler: Running (status, disabled)
└── Quit Tether (⌘Q)
```

## Behavior

### Window Management
- **Close Window (⌘W)** - Hides window, app continues running in tray
- **Quit Tether (⌘Q)** - Fully quits the application (from tray menu)
- Closing all windows doesn't quit the app - scheduler keeps running

### Recent Projects
- Shows 3 most recently opened projects
- Click behavior:
  - If project already open in a window → focus that window
  - If project not open → create new window and open project
- Updated when projects are opened/created

### Project Actions
- **Create Project** - Opens dialog to create new project, opens in new window
- **Open Project** - Opens file picker to select existing project, opens in new window

### Navigation Items
- **View Runs** - Opens run history view
  - If project already open → navigate to runs view in that window
  - If no project open → open empty project and navigate to runs view
- **View Scheduler** - Opens scheduler view
  - If project already open → navigate to scheduler view in that window
  - If no project open → open empty project and navigate to scheduler view

### MCP Management
- **Install MCP** - Opens new window for managing Tether MCP servers
  - Browse available MCPs
  - Install from directory
  - Manage installed MCPs

### Status Indicators
- **Scheduler Status** - Read-only indicator showing scheduler state
  - "Scheduler: Running" (green/default)
  - "Scheduler: Paused" (amber)
  - "Scheduler: Error" (red)

## Implementation

### Location
- `src-tauri/src/lib.rs` - Tray icon setup and event handling
- `src-tauri/src/tray.rs` (future) - Dedicated tray module with state management

### Key Components

1. **Tray Icon Builder** - Creates persistent tray icon
2. **Menu Construction** - Builds dynamic menu with recent projects
3. **Event Handler** - Responds to menu item clicks
4. **Window Manager** - Handles window creation, focusing, and navigation

### State Management

Recent projects stored in:
- `~/.tether/recent_projects.json` - Global recent projects list
- Contains: project name, path, last opened timestamp
- Max 3 entries, sorted by most recent

### Platform Behavior
- **macOS** - Menu bar icon, standard tray behavior
- **Windows** - System tray icon in notification area
- **Linux** - System tray support varies by desktop environment

## Technical Details

### Tray Icon Persistence
The tray icon is created in the `setup` hook and persists for the app lifetime. It's only destroyed when the user selects "Quit Tether".

### Window Close vs App Quit
```rust
main_window.on_window_event(move |event| {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        // Hide window instead of closing app
        let _ = window_clone.hide();
        api.prevent_close();
    }
});
```

This ensures the scheduler and background tasks continue running.

### Menu Item IDs
- `tray_open` - Open/show main window
- `tray_create_project` - Create new project
- `tray_open_project` - Open existing project
- `tray_recent_{index}` - Recent project items (dynamic)
- `tray_view_runs` - Open run history
- `tray_view_scheduler` - Open scheduler
- `tray_install_mcp` - Open MCP manager
- `tray_scheduler_status` - Scheduler status (disabled)
- `tray_quit` - Quit application

## Future Enhancements

- Show number of queued/running jobs in tray icon badge
- Quick actions for pausing/resuming scheduler
- Notifications for completed/failed runs
- Per-project tray icons when managing multiple projects
