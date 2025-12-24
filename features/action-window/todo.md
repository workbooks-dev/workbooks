# Action Window - Implementation Tasks

The Action Window is the central launcher for Workbooks - the entry point for all operations.

See `docs.md` for full design specification.

## Core Implementation

### Backend (Rust)
- [x] Read recent_projects.rs and verify get_recent_projects command exists
- [x] Add get_recent_projects Tauri command (lib.rs:273)
- [x] Register command in invoke_handler (lib.rs:1519)
- [x] Add get_all_runs command for cross-project run history (already exists via list_runs_paginated)
- [x] Add get_all_schedules command for cross-project schedule view (already exists via list_schedules)
- [x] Test recent projects tracking integration

### Frontend (React)
- [x] Create ActionWindow.jsx component
  - [x] Layout with logo and centered container
  - [x] Recent Projects section (top 3)
  - [x] Projects section (Create/Open buttons)
  - [x] Global Views section (All Runs/All Schedules)
  - [x] Settings section (Install MCP button)
  - [x] Empty states for no recent projects
  - [x] Loading states for async operations
- [x] Style according to STYLE_GUIDE.md (grayscale + blue, minimal, clean)
- [x] Add hover states and transitions
- [ ] Make all sections keyboard accessible (Command+1/2/3 for recent projects)

### Integration
- [x] Update App.jsx to show ActionWindow as initial view
- [x] Add routing logic: ActionWindow → Project Window on project select
- [x] Implement "transform window" behavior (reuse same window)
- [x] Wire up all action buttons to Tauri commands
- [x] Handle project opening loads the project properly
- [ ] Add Command+N shortcut to open new Action Window
- [ ] Check if project is already open before opening again

### Tray Integration
- [x] Tray menu handlers work with Action Window (tray events in App.jsx)
- [x] Update "View Runs" tray handler to use global runs view (App.jsx:189-192)
- [x] Update "View Scheduler" tray handler to use global schedules view (App.jsx:195-198)
- [ ] Test complete tray → Action Window → Project flow

## Future Enhancements
- [ ] Command+K quick search/command palette
- [ ] Project templates
- [ ] Pinned/favorite projects
- [ ] Quick stats display
- [ ] Onboarding for first-time users
- [ ] Recent activity feed
