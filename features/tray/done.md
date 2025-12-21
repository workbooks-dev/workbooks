# Tray Menu - Completed

## Backend Implementation (Rust)

- [x] Created `src-tauri/src/recent_projects.rs` module
  - Stores recent projects in `~/.tether/recent_projects.json`
  - Maintains max 3 recent projects, sorted by last opened
  - Auto-updates when projects are created/opened/loaded

- [x] Updated system tray menu in `src-tauri/src/lib.rs`
  - Recent projects section (dynamic, max 3)
  - "Create Project..." menu item
  - "Open Project..." menu item
  - "View Runs" menu item
  - "View Scheduler" menu item
  - "Install MCP..." menu item
  - "Scheduler: Running" status indicator (disabled)
  - "Quit Tether" menu item

- [x] Tray event handler
  - Emits `open-project` event for recent project clicks
  - Emits `tray-create-project` event
  - Emits `tray-open-project` event
  - Emits `tray-view-runs` event
  - Emits `tray-view-scheduler` event
  - Emits `tray-install-mcp` event
  - Handles quit action

- [x] Window close behavior
  - Windows hide instead of quitting app
  - App continues running in background
  - Scheduler keeps running when windows closed
  - **Fixed: Tray menu behavior (Dec 21, 2025)**
    - Tray menu items now work when all windows are hidden
    - **Discovery**: Hidden windows are completely removed from Tauri's window HashMap
    - **Solution**: Two-path handling in src-tauri/src/lib.rs:
      - If window exists: Show it and emit navigation event
      - If no window exists: Create new window with URL parameters
    - **Window Creation**: Added `create_main_window()` helper
      - Accepts optional `view` parameter (e.g., "global-schedules", "global-runs")
      - Builds URL with query parameters: `index.html?view={view}`
      - Configures close handler to hide instead of quit
    - **URL-based Navigation**: App.jsx reads URL parameters on mount
      - Parses `?view=` parameter to navigate to correct view
      - Parses `?project=` parameter to load specific project
      - Works for both new windows and existing windows
    - **React Component Keys**: Added unique keys to force remounting
      - `key="global-schedules"` for schedules view
      - `key="global-runs"` for runs view
      - Ensures data updates when switching between views via tray
    - **Fixed: Opening tray menu no longer resets window state**
      - Removed tray icon click handler to prevent unwanted resets
      - Menu can be browsed without affecting application state
      - Only selecting menu items triggers actions
      - Standard macOS tray menu behavior

- [x] Recent projects tracking
  - Added tracking to `create_project` command
  - Added tracking to `open_folder` command
  - Added tracking to `load_project` command

## Frontend Implementation (React)

- [x] Added tray event listeners in `src/App.jsx`
  - `open-project` - Opens recent project from tray
  - `tray-create-project` - Shows create project dialog
  - `tray-open-project` - Shows open project dialog
  - `tray-view-runs` - Navigates to schedule tab (runs view)
  - `tray-view-scheduler` - Navigates to schedule tab
  - `tray-install-mcp` - Shows MCP installation placeholder

- [x] Window management integration
  - Tray events show/focus window if exists
  - Create new window with view parameter if none exist
  - Navigate to appropriate views when window already exists
  - Event handlers clear project state before navigating to global views
  - React keys force component remounting when switching views

## Documentation

- [x] Created `features/tray/docs.md` with full design documentation
- [x] Created `features/tray/done.md` (this file)
