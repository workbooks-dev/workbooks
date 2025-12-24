# Action Window - Completed Features

All completed items will be listed here chronologically as they are implemented.

## 2025-12-20

### Feature Documentation
- Created feature documentation structure (docs.md, todo.md, done.md)
- Designed Action Window UI layout and behavior
- Documented all use cases, UI patterns, and integration points

### Backend Implementation
- Added `get_recent_projects` Tauri command in src-tauri/src/lib.rs:273
- Registered command in invoke_handler (lib.rs:1519)
- Integrated with existing recent_projects.rs module
- Command returns Vec<RecentProject> from ~/.workbooks/recent_projects.json

### Frontend Implementation
- Created ActionWindow.jsx component (src/components/ActionWindow.jsx)
- Implemented clean, centered layout with Workbooks branding
- Added Recent Projects section (displays top 3 recent projects)
- Added Projects section (Create Project, Open Project buttons)
- Added Global Views section (View All Runs, View All Schedules placeholders)
- Added Settings section (Install MCP placeholder)
- Implemented empty states and loading states
- Followed STYLE_GUIDE.md patterns (grayscale + blue accents, minimal design)

### App Integration
- Integrated ActionWindow into App.jsx routing
- Changed default view from "welcome" to "action"
- Added handleActionWindowAction function to process user actions
- Updated Create Project back button to return to Action Window
- Updated error handling to return to Action Window instead of Welcome
- Wired up all action handlers (create-project, open-project, view-all-runs, view-all-schedules)
- Added global-schedules and global-runs views in App.jsx (App.jsx:689-735)
- These views use ScheduleTab component with projectRoot=null and showAllProjects=true
- Updated tray event listeners to navigate to global views instead of showing alerts (App.jsx:189-198)

### Testing
- Successfully tested app startup in dev mode
- Verified Vite dev server starts correctly
- Confirmed Rust backend compiles without errors
- Validated system tray and menu bar initialization
- Action Window displays as the default entry point

### What Works
✅ Action Window appears on app startup (when no project is loaded)
✅ Recent projects are fetched and displayed from ~/.workbooks/recent_projects.json
✅ Create Project button navigates to project creation flow
✅ Open Project button opens native folder picker
✅ Recent project clicks load the selected project
✅ Back button from Create Project returns to Action Window
✅ Clean, professional UI following Workbooks design guidelines
✅ View All Runs button opens global runs view (all projects)
✅ View All Schedules button opens global schedules view (all projects)
✅ Both global views reuse ScheduleTab component with showAllProjects=true

### Global Views Integration (Dec 20, 2025)
✅ View All Schedules opens global scheduler view showing all projects
✅ View All Runs opens global runs history showing all projects
✅ Enhanced ScheduleTab component to support global mode (ScheduleTab.jsx:5-15)
  - Made projectRoot optional (defaults to null)
  - Added initialSubTab prop to set starting tab ("scheduled" or "runs")
  - Added initialShowAllProjects prop to control global vs project view
  - When projectRoot is null, automatically enables showAllProjects
✅ Added global-schedules and global-runs views to App.jsx (App.jsx:689-735)
  - Both views render ScheduleTab with appropriate configuration
  - Include back button to return to Action Window
  - Use same header style as other views
✅ Updated tray menu handlers to navigate to global views (App.jsx:189-198)
  - "View Scheduler" tray item now opens global-schedules view
  - "View Runs" tray item now opens global-runs view
  - Removed placeholder alerts, now shows actual functionality
✅ Updated window lifecycle handlers for global views (App.jsx:354-356, 391-396)
  - Closing window in global views hides window (doesn't quit app)
  - Command+W in global views hides window
  - Same behavior as Action Window

### Window Lifecycle Management (Dec 20, 2025)
✅ Closing project windows returns to Action Window (doesn't quit app)
✅ Command+W in project view closes last tab and returns to Action Window
✅ Command+W in Action Window hides the window (app stays running in tray)
✅ Closing Action Window hides the window (app stays running in tray)
✅ App only quits when "Quit Workbooks" is selected from tray menu
✅ Window close with unsaved changes shows save dialog, then returns to Action Window
✅ Proper state cleanup when returning to Action Window (project, tabs, activeTabId all cleared)

**Implementation Details:**
- Updated App.jsx onCloseRequested handler to check current view (App.jsx:353-377)
  - When in project/create view: prevents close and resets to Action Window
  - When in Action Window/global views: allows close (lib.rs hides window and keeps app running)
- Updated Command+W handler to hide window for global views (App.jsx:391-396)
- Updated Command+W handler to reset to Action Window when closing last tab (App.jsx:398-434)
- Updated save dialog handlers to support 'reset-to-action' flow (App.jsx:615-664)
- lib.rs window close handler hides window instead of closing app (lib.rs:1351-1363)
- **CRITICAL FIX:** Added RunEvent::ExitRequested handler to prevent app quit (lib.rs:1619-1628)
  - Calls `api.prevent_exit()` to keep app running when all windows are closed
  - Updated tray quit handler to use `std::process::exit(0)` for force quit (lib.rs:1366-1368)
  - This ensures the app only quits when explicitly selected from tray menu
  - **Result:** Closing all windows now hides them and keeps app running in tray ✅
- **CRITICAL FIX:** Fixed tray menu items not working when window is hidden (lib.rs:1298-1414)
  - All tray menu event handlers now show and focus window before emitting events
  - **Added 100ms delay after showing window** to allow React app to initialize event listeners
  - Recent project clicks show window, wait, then emit event (lib.rs:1320-1334)
  - Create/Open/View Runs/View Scheduler all show window, wait, then emit (lib.rs:1342-1405)
  - Added tray icon click handler to show/focus window (lib.rs:1298-1307)
  - **Result:** All tray menu items now work correctly when window is hidden ✅
- **CRITICAL FIX:** Fixed event listener dependencies causing tray events to be ignored (App.jsx:140-220)
  - Changed useEffect dependencies from `[currentProject, tabs]` to `[]`
  - This keeps listeners active for the app's entire lifetime instead of being torn down
  - **Result:** Tray menu events are now properly received and processed ✅
