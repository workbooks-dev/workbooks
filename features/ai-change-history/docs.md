# AI Change History & Undo System

## Overview

A git-free change tracking system that allows users to undo AI agent modifications without forcing git workflows. Stores change history locally with automatic cleanup based on age and size limits.

## Problem

Currently, when the AI agent modifies files using Edit, Write, or Bash tools, changes are applied directly to disk with no history or ability to undo. Users need confidence that they can safely experiment with the agent and roll back changes if needed.

## Architecture

### Storage Structure

```
.tether/
├── chat_sessions.db          # Existing
├── agent_changes.db          # NEW: Change metadata
└── snapshots/                # NEW: File snapshots
    ├── {change_id}/
    │   ├── before/
    │   │   └── src/foo.js    # Pre-change snapshot
    │   └── after/
    │       └── src/foo.js    # Post-change snapshot
    └── ...
```

### Database Schema

```sql
-- agent_changes.db
CREATE TABLE change_sets (
  id TEXT PRIMARY KEY,              -- UUID
  session_id TEXT NOT NULL,         -- Link to chat session
  timestamp INTEGER NOT NULL,
  description TEXT,                 -- AI-generated summary
  tool_count INTEGER DEFAULT 0,
  can_rollback BOOLEAN DEFAULT 1    -- False if files changed externally
);

CREATE TABLE file_changes (
  id INTEGER PRIMARY KEY,
  change_set_id TEXT NOT NULL,
  file_path TEXT NOT NULL,          -- Relative to project root
  operation TEXT NOT NULL,          -- 'create', 'edit', 'delete', 'bash'
  tool_name TEXT NOT NULL,          -- 'Write', 'Edit', 'Bash'
  snapshot_path TEXT,               -- Path to before/after snapshots
  can_rollback BOOLEAN DEFAULT 1,
  FOREIGN KEY (change_set_id) REFERENCES change_sets(id)
);

CREATE INDEX idx_change_sets_session ON change_sets(session_id);
CREATE INDEX idx_file_changes_changeset ON file_changes(change_set_id);
```

## How It Works

### 1. Tool Call Interception

Wrap the Claude Agent SDK in `engine_server.py` to capture tool usage before execution:

```python
class ChangeTrackingAgent:
    def __init__(self, project_root):
        self.project_root = project_root
        self.current_change_set = None

    def start_change_set(self, session_id):
        """Begin tracking a new set of changes"""
        self.current_change_set = {
            'id': str(uuid.uuid4()),
            'session_id': session_id,
            'timestamp': int(time.time()),
            'files': []
        }

    def track_tool_call(self, tool_name, args, result):
        """Intercept before tool execution"""
        if tool_name in ['Edit', 'Write']:
            file_path = args.get('file_path')
            self.snapshot_before(file_path)
            # Execute tool...
            self.snapshot_after(file_path)
            self.record_change(tool_name, file_path, 'edit' if tool_name == 'Edit' else 'create')

        elif tool_name == 'Bash':
            # Track bash commands that might modify files
            cmd = args.get('command')
            if is_file_modifying_command(cmd):
                self.track_bash_changes(cmd)
```

### 2. Snapshot Strategy

**Hybrid Approach:**
- **Files < 100KB**: Full snapshots (fast, simple rollback)
- **Files > 100KB**: Git-style diffs (efficient storage)
- **Binary files**: Full snapshots only

**Full Snapshot**: Store complete file contents before/after
- Easy rollback: just copy the "before" version back
- Disk usage: ~2x file size per change
- Best for small files

**Diff Storage**: Store only the unified diff
- Rollback: apply reverse patch
- Disk usage: Usually < 10% of file size
- Better for large files

### 3. Retention Policy

```python
class ChangeRetentionPolicy:
    MAX_AGE_DAYS = 30
    MAX_TOTAL_SIZE_MB = 500  # Configurable
    MIN_KEEP_CHANGES = 10    # Always keep last 10

    def cleanup_old_changes(self):
        """Run periodically (on app start, after each session)"""
        # 1. Delete changes older than 30 days
        cutoff = time.time() - (self.MAX_AGE_DAYS * 86400)
        old_changes = get_changes_before(cutoff)

        # Keep minimum number of recent changes
        if len(all_changes) - len(old_changes) >= self.MIN_KEEP_CHANGES:
            delete_change_sets(old_changes)

        # 2. If still over size limit, delete oldest first
        while get_total_snapshot_size() > self.MAX_TOTAL_SIZE_MB * 1024 * 1024:
            oldest = get_oldest_change_set()
            if oldest:
                delete_change_set(oldest)
            else:
                break
```

Cleanup runs:
- On app startup
- After each agent session completes
- Can be triggered manually from settings

## UI/UX Design

### In AI Sidebar (After Agent Response)

```
┌─────────────────────────────────┐
│ 🤖 AI Assistant                 │
├─────────────────────────────────┤
│ [Chat messages...]              │
│                                 │
│ ✓ Assistant: I've updated 3     │
│   files for you.                │
│                                 │
│   📝 Changes:                   │
│   • src/App.jsx (edited)        │
│   • src/hooks/useProject.js     │
│   • tests/App.test.js (created) │
│                                 │
│   [👁️ Review] [↶ Undo All]      │
└─────────────────────────────────┘
```

### Change History View

New tab or sidebar section showing all tracked changes:

```
┌─────────────────────────────────┐
│ 📜 Change History               │
├─────────────────────────────────┤
│ Today                           │
│ • 2:34 PM - "Added test suite"  │
│   3 files • [View] [Undo]       │
│                                 │
│ • 11:22 AM - "Fixed bug in..."  │
│   1 file • [View] [Undo]        │
│                                 │
│ Yesterday                       │
│ • 4:15 PM - "Refactored auth"   │
│   5 files • [View] [Undo]       │
└─────────────────────────────────┘
```

### Change Detail Modal

```
┌──────────────────────────────────────┐
│ Change Set: "Added test suite"       │
│ 2:34 PM • Session: "Build tests"     │
├──────────────────────────────────────┤
│ Files changed (3):                   │
│                                      │
│ ✓ src/App.jsx                        │
│   • Added useEffect hook             │
│   • [View Diff] [Undo This File]     │
│                                      │
│ ✓ src/hooks/useProject.js            │
│   • Modified error handling          │
│   • [View Diff] [Undo This File]     │
│                                      │
│ ✓ tests/App.test.js (created)        │
│   • [View File] [Undo This File]     │
│                                      │
│ [Undo All Changes] [Close]           │
└──────────────────────────────────────┘
```

## Safety Features

### 1. External Change Detection

Before rollback, verify file hasn't been modified outside the agent:

```python
def can_rollback(change):
    current_content = read_file(change.file_path)
    snapshot_after = read_snapshot(change.snapshot_path + '/after')
    return current_content == snapshot_after
```

### 2. Conflict Warnings

```
⚠️ Warning: src/App.jsx has been modified since
this change was made. Rolling back may cause
unexpected results.

[Cancel] [Force Rollback Anyway]
```

### 3. Grouped Rollbacks

- Undo all changes from a single conversation session
- Atomic rollbacks: if multi-file rollback fails, restore all to original state

### 4. Rollback Validation

- Check file exists before rollback
- Verify snapshots are intact
- Handle missing files gracefully (file was deleted externally)

## Configuration

Settings exposed in project settings UI and config file:

```toml
# .tether/config.toml or global settings
[agent.changes]
enabled = true
max_age_days = 30
max_size_mb = 500
min_keep_count = 10
snapshot_strategy = "hybrid"  # "full", "diff", or "hybrid"
```

## Design Decisions

### Why not use git?

**Problem**: Forcing git creates friction for users who:
- Don't use version control
- Use other VCS systems (SVN, Mercurial)
- Are beginners and find git intimidating
- Work with non-code files (data science notebooks)

**Solution**: Built-in change tracking that "just works" without external dependencies.

### Why snapshots instead of git internals?

- **Simpler**: No git dependency, works everywhere
- **Transparent**: Users understand "before/after" snapshots
- **Flexible**: Can track any file type, not just text
- **Isolated**: Doesn't interfere with existing git workflows

### Why hybrid snapshot strategy?

- Small files: Full snapshots are fast and simple
- Large files: Diffs save significant disk space
- Binary files: Diffs don't work, need full snapshots
- Gives best balance of simplicity and efficiency

### Why 30-day retention default?

- Long enough to cover "oops, I need that from last week"
- Short enough to prevent unbounded disk growth
- User can configure longer if needed
- Always keeps minimum 10 recent changes regardless of age

### Why track at change-set level?

- Groups related changes from single agent interaction
- Allows "undo everything from this conversation"
- Matches user mental model: "undo what the AI just did"
- Easier to review changes in context

## Technical Implementation Notes

### Rust Side (Tauri Commands)

New commands needed:
- `get_change_history()` - Fetch all change sets
- `get_change_set_details(change_set_id)` - Get files in a change set
- `rollback_change_set(change_set_id)` - Undo all changes
- `rollback_file_change(file_change_id)` - Undo single file
- `get_file_diff(file_change_id)` - Get before/after diff
- `cleanup_old_changes()` - Run retention policy

### Python Side (Engine Server)

Modifications to `engine_server.py`:
- Wrap agent tool execution in tracking layer
- Snapshot files before/after modifications
- Write change metadata to SQLite
- Detect Bash commands that modify files
- Generate change set descriptions (optional: ask Claude to summarize)

### Frontend (React)

New components:
- `ChangeHistory.jsx` - Main history view
- `ChangeSetDetail.jsx` - Detail modal with diffs
- `ChangesSummary.jsx` - Inline summary in chat
- Update `AiSidebar.jsx` to show change summaries

### Performance Considerations

- Snapshots written asynchronously (don't block agent)
- Cleanup runs in background thread
- Large files use streaming for snapshots
- SQLite indexes for fast queries
- Lazy-load diffs only when viewing

## Future Enhancements

- Export change history to git commits (opt-in bridge)
- Visual diff viewer with syntax highlighting
- "Stash" feature: temporarily revert, then restore
- Change annotations in file viewer (show what agent changed)
- Partial rollbacks (undo specific lines, not whole file)
- Cloud sync for change history (team collaboration)
- AI-generated change summaries and commit messages
