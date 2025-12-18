# Navigation - To Do

## High Priority

- [ ] Add new tab types for management views:
  - [ ] `workbooks-table` - Full workbook table view (when clicking "Workbooks" header)
  - [ ] `secrets` - Secrets management interface
  - [ ] `schedule` - Schedule management (with tabs for Scheduled Workbooks / Recent Runs)
  - [ ] `settings` - Project settings

- [ ] Replace modal-based UIs with tab-based UIs:
  - [ ] Workbooks table view (currently modal) → Tab
  - [ ] Project settings (currently planned as modal) → Tab
  - [ ] Secrets UI (not built yet) → Tab
  - [ ] Schedule UI (not built yet) → Tab

- [ ] Prevent duplicate tabs:
  - [ ] Check if tab already exists before opening
  - [ ] Switch to existing tab instead of creating duplicate
  - [ ] Use tab ID or path for deduplication

## Medium Priority

- [ ] Tab persistence:
  - [ ] Save open tabs to localStorage on close
  - [ ] Restore tabs on app restart
  - [ ] Handle missing files gracefully

- [ ] Tab reordering:
  - [ ] Drag and drop to reorder tabs
  - [ ] Remember tab order

- [ ] Keyboard shortcuts:
  - [ ] Cmd/Ctrl+W to close current tab
  - [ ] Cmd/Ctrl+Tab to cycle through tabs
  - [ ] Cmd/Ctrl+1-9 to jump to specific tab

## Low Priority

- [ ] Tab groups or separators for organization
- [ ] Recently closed tabs history (Cmd+Shift+T to reopen)
- [ ] Pin important tabs (prevent accidental close)
