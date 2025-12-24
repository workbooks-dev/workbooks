# AI Change History - TODO

## Phase 1: Basic Tracking (1-2 days)

### Database & Storage
- [ ] Create `agent_changes.db` schema in Rust
  - [ ] Add `change_sets` table
  - [ ] Add `file_changes` table
  - [ ] Add indexes for session and change set lookups
- [ ] Create `.workbooks/snapshots/` directory structure
- [ ] Add Rust module for change tracking database operations
  - [ ] Insert change sets
  - [ ] Insert file changes
  - [ ] Query change history
  - [ ] Check rollback eligibility

### Tool Interception (Python)
- [ ] Create `ChangeTrackingAgent` wrapper class in `engine_server.py`
- [ ] Intercept `Write` tool calls
  - [ ] Snapshot file before (if exists)
  - [ ] Execute Write tool
  - [ ] Snapshot file after
  - [ ] Record change in database
- [ ] Intercept `Edit` tool calls
  - [ ] Snapshot file before
  - [ ] Execute Edit tool
  - [ ] Snapshot file after
  - [ ] Record change in database
- [ ] Start/end change set per agent interaction
  - [ ] Create change set ID when `/agent/chat` starts
  - [ ] Finalize change set when interaction completes
- [ ] Store full file snapshots (before/after) for all files

### Retention & Cleanup
- [ ] Implement `ChangeRetentionPolicy` class
- [ ] Run cleanup on app startup
- [ ] Default retention: 30 days, 500MB max, keep last 10 changes
- [ ] Delete old snapshots when pruning change sets

## Phase 2: UI Integration (2-3 days)

### Change Summary in Chat
- [ ] Create `ChangesSummary.jsx` component
- [ ] Show inline summary after agent responses
  - [ ] List files modified/created/deleted
  - [ ] Show file count and operation types
  - [ ] Add "Review" and "Undo All" buttons
- [ ] Update `AiSidebar.jsx` to fetch and display changes per message
- [ ] Style to match existing UI (minimal, grayscale + blue)

### Rollback Commands
- [ ] Add Tauri command: `rollback_change_set(change_set_id)`
  - [ ] Read change set from database
  - [ ] Verify all files can be rolled back (no external modifications)
  - [ ] Copy "before" snapshots back to original locations
  - [ ] Handle created files (delete them)
  - [ ] Mark change set as rolled back in database
- [ ] Add Tauri command: `can_rollback_change_set(change_set_id)`
  - [ ] Check each file against "after" snapshot
  - [ ] Return list of files with conflicts
- [ ] Add confirmation modal before rollback
- [ ] Show success/error messages after rollback
- [ ] Handle rollback errors gracefully

### Testing
- [ ] Test with single file edits
- [ ] Test with multiple file changes
- [ ] Test with file creation
- [ ] Test external modification detection
- [ ] Test rollback with conflicts
- [ ] Test retention cleanup

## Phase 3: Enhanced Features (2-3 days)

### Bash Tool Tracking
- [ ] Detect file-modifying bash commands
  - [ ] Parse commands for file operations (mv, cp, rm, sed, etc.)
  - [ ] Track affected files
  - [ ] Snapshot before/after if possible
- [ ] Mark bash changes with uncertainty flag (may not capture all files)
- [ ] Show bash commands in change history

### Change History View
- [ ] Create `ChangeHistory.jsx` component
- [ ] Add new sidebar section or tab for history
- [ ] Group changes by date (Today, Yesterday, This Week, etc.)
- [ ] Show change set descriptions and timestamps
- [ ] Show file count per change set
- [ ] Add search/filter by file name or description
- [ ] Add "View" and "Undo" actions per change set

### Change Detail Modal
- [ ] Create `ChangeSetDetail.jsx` component
- [ ] Show all files in change set
- [ ] List operations per file (created, edited, deleted)
- [ ] Add "View Diff" button per file
- [ ] Add "Undo This File" button (granular rollback)
- [ ] Add "Undo All Changes" button
- [ ] Show conflict warnings if files modified externally

### Diff Viewer
- [ ] Add Tauri command: `get_file_diff(file_change_id)`
  - [ ] Read before/after snapshots
  - [ ] Generate unified diff
  - [ ] Return diff text
- [ ] Create `DiffViewer.jsx` component
- [ ] Show before/after side-by-side or unified view
- [ ] Syntax highlight diffs
- [ ] Handle binary files (show "binary file changed" message)

### Granular Rollback
- [ ] Add Tauri command: `rollback_file_change(file_change_id)`
- [ ] Allow rolling back individual files
- [ ] Update change set status if partially rolled back
- [ ] Show which files were rolled back in history

### External Modification Detection
- [ ] Check file contents vs "after" snapshot before rollback
- [ ] Show warning modal if conflicts detected
- [ ] Offer "Force Rollback Anyway" option
- [ ] Mark conflicted files in change history UI

## Phase 4: Polish (1-2 days)

### Hybrid Snapshot Strategy
- [ ] Implement file size detection (< 100KB vs > 100KB)
- [ ] Add full snapshot mode for small files
- [ ] Add diff-based storage for large files
  - [ ] Generate unified diffs
  - [ ] Store diffs instead of full contents
  - [ ] Apply reverse patches on rollback
- [ ] Handle binary files (always full snapshots)
- [ ] Make strategy configurable in settings

### AI-Generated Change Descriptions
- [ ] After change set completes, ask Claude to summarize
  - [ ] Pass list of files and operations
  - [ ] Get concise description (1-2 sentences)
  - [ ] Store in change set metadata
- [ ] Fall back to generic descriptions if summarization fails
- [ ] Show loading state while generating description

### Configuration UI
- [ ] Add "Change History" section to project settings
- [ ] Toggle: Enable/disable change tracking
- [ ] Input: Max age in days (default 30)
- [ ] Input: Max total size in MB (default 500)
- [ ] Input: Minimum changes to keep (default 10)
- [ ] Dropdown: Snapshot strategy (full, diff, hybrid)
- [ ] Button: "Clean Up Now" (run retention policy manually)
- [ ] Show current disk usage stats

### Background Cleanup Task
- [ ] Run cleanup after each agent session completes
- [ ] Run cleanup on app startup
- [ ] Add progress indicator if cleanup is slow
- [ ] Log cleanup results (files deleted, space freed)

### Settings Persistence
- [ ] Store config in `.workbooks/config.toml` or global settings
- [ ] Load config on app startup
- [ ] Apply retention policy based on user settings
- [ ] Validate settings (min > 0, reasonable size limits)

## Future Enhancements (Backlog)

- [ ] Export change history to git commits (opt-in)
- [ ] Visual diff viewer with syntax highlighting (Monaco-based)
- [ ] "Stash" feature: temporarily revert, then restore later
- [ ] Change annotations in file viewer (show what agent changed)
- [ ] Partial rollbacks (undo specific lines, not whole file)
- [ ] Cloud sync for change history (team collaboration)
- [ ] Change set merging (combine related changes)
- [ ] Change set diffing (compare two change sets)
- [ ] Keyboard shortcuts for undo (Cmd+Z for last agent change)
- [ ] Undo history (undo the undo)
