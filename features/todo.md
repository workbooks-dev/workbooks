# Workbooks High-Level Roadmap

This file tracks the high-level feature priorities and cross-cutting tasks. For detailed implementation todos, see individual feature directories.

## Opening a Workbooks project

- [ ] if a pyproject.toml exists, do not duplicate "dependency-groups", add to it


## Current Status Summary (Updated Dec 20, 2025)

### ✅ Completed Core Features
- **Workbooks System** (~95% MVP complete) - Full execution, streaming output, rich rendering, keyboard shortcuts, autosave
- **Secrets Management** (~90% complete) - Full encryption, CRUD UI, kernel injection, output warning system, Touch ID auth
- **Schedule System** (~85% complete) - Backend scheduler, CLI commands, frontend UI, run history, pagination, date filters, execution insights
- **Files Management** - Complete CRUD operations, image/CSV/JSON viewers, drag-and-drop, context menus, subfolder support
- **Navigation** - Tab system, native macOS menu bar, multi-file support
- **Sidebar** - Multi-section layout (Workbooks, Secrets, Schedule, Files, Settings)
- **Python/uv Integration** - Virtual environments, package management, centralized venv storage
- **UI System** - Complete style guide, professional grayscale + blue aesthetic, consistent design patterns
- **System Tray** - Background process, window hide/show, scheduler continues when window closed

### 🔴 Critical Issues (Fix First)

✅ **All critical issues resolved!** (Completed Dec 19, 2024)

---

## Recommended Next Implementation Steps (Tier 1 - High Impact)

Based on recent audit of workbooks, schedule, and secrets features:

### 1. **Schedule: Pending Event Display & Cancellation** 🌟
**Goal:** Show next scheduled run and allow user to cancel it
**Status:** 🟡 High Priority
**Details:** See `features/schedule/todo.md` (top item)

**Why:** Users want visibility into upcoming runs and ability to skip them. Scheduler works great but lacks this UI.

**Key Tasks:**
- [ ] Display next scheduled run in Schedule tab header
- [ ] "Cancel Next Run" button to skip the upcoming execution
- [ ] Re-calculate next run after cancellation
- [ ] Show countdown timer to next run

**Estimated Effort:** Medium (1-2 days)

---

### 2. **Secrets: Touch ID Session Management UI** 🔒
**Goal:** Visual lock/unlock indicator and manual lock button
**Status:** 🟡 High Priority
**Details:** See `features/secrets/todo.md`

**Why:** Session management exists in backend but no UI feedback. Users can't tell if secrets are locked or unlocked.

**Key Tasks:**
- [ ] Lock/unlock status indicator in Secrets tab header
- [ ] "Lock Secrets" button to manually invalidate session
- [ ] Session expiry on app close (currently persists until timeout)
- [ ] Visual feedback when session expires

**Estimated Effort:** Small (0.5-1 day)

---

### 3. **Schedule: Workbooks Table Integration** 📋
**Goal:** Quick access to scheduling from workbooks list
**Status:** 🟢 Ready to Build
**Details:** See `features/schedule/todo.md`

**Why:** Makes scheduling more discoverable. Currently users must open Schedule tab first.

**Key Tasks:**
- [ ] "Schedule" button in workbooks table Actions column
- [ ] Opens Add Schedule dialog pre-filled with workbook name
- [ ] Show scheduled indicator badge if workbook is already scheduled
- [ ] Display frequency/next run in workbook row

**Estimated Effort:** Medium (1-2 days)

---

## Recommended Next Steps (Tier 2 - Polish & UX)

### 4. **Schedule: Sidebar Enhancements**
**Goal:** Better at-a-glance visibility of schedules
**Details:** See `features/schedule/todo.md`

**Key Tasks:**
- [ ] Show scheduled workbook count in sidebar
- [ ] Show next upcoming run time
- [ ] Visual indicator for active schedules

**Estimated Effort:** Small (0.5 day)

---

### 5. **Schedule: Run Report Viewer**
**Goal:** View executed notebooks with saved outputs
**Status:** 🟡 Medium Priority
**Details:** See `features/schedule/todo.md`

**Why:** Critical for debugging failed runs. Currently only see metadata, not full outputs.

**Key Tasks:**
- [ ] Save executed notebook with outputs to `.workbooks/runs/{run_id}.ipynb`
- [ ] "View Report" button in Recent Runs tab
- [ ] Open report in read-only tab
- [ ] Display saved notebook with all outputs preserved
- [ ] Cannot edit cells (read-only mode)

**Estimated Effort:** Large (3-4 days)

---

### 6. **Workbooks: Execution State Persistence**
**Goal:** Preserve unsaved cell changes when switching tabs
**Status:** 🟡 Medium Priority
**Details:** See `features/workbooks/todo.md`

**Why:** Current behavior (reverting to saved state) is confusing. Users lose work when switching tabs.

**Key Tasks:**
- [ ] Store execution state (cell outputs, unsaved changes) in memory
- [ ] Restore state when returning to tab
- [ ] Clear state only on explicit save or discard

**Estimated Effort:** Medium (2-3 days)

---

## Recommended Next Steps (Tier 3 - Advanced Features)

### 7. **Secrets: Auto-Detection of Hardcoded Secrets**
**Goal:** Proactively detect secrets before execution
**Status:** 🟢 Enhancement
**Details:** See `features/secrets/todo.md`

**Why:** Prevent secrets from being hardcoded in the first place.

**Key Tasks:**
- [ ] Pattern recognition (API keys, tokens, entropy analysis)
- [ ] "Detected secret" dialog before cell execution
- [ ] One-click migration to secrets manager
- [ ] Auto-rewrite cell to use `os.environ["KEY"]`

**Estimated Effort:** Large (4-5 days)

---

### 8. **Schedule: System Tray Dynamic Updates**
**Goal:** Live tray menu with schedule status
**Status:** 🟢 Enhancement
**Details:** See `features/schedule/todo.md`

**Why:** Nice-to-have. Basic tray already works; this adds polish.

**Key Tasks:**
- [ ] Update status text when schedules run
- [ ] Show countdown to next run in menu
- [ ] Pause/Resume scheduler from tray
- [ ] Dynamic icon (idle/running/error states)

**Estimated Effort:** Medium-Large (2-3 days)

---

## Current MVP Completion Tasks

### 1. Network Status & Offline Behavior
**Goal:** Clear user feedback when internet is required
**Status:** 🟡 Medium-High Priority
**Details:** See `features/network/todo.md`

**Key Tasks:**
- [ ] Network status indicator (online/offline)
- [ ] Clear error messages when offline
- [ ] Progress indicators for downloads (uv, packages)
- [ ] Retry mechanism for failed network operations

**Why:** Backend network operations exist but have no UI feedback. Users are confused during first-time setup.

---

### 2. Tab-Based Navigation for Management Views
**Goal:** Replace modals with tabs for consistency
**Status:** 🟡 Medium Priority
**Details:** See `features/navigation/todo.md`

**Key Tasks:**
- [ ] Add tab types: workbooks-table, settings
- [ ] Replace Workbooks table modal with tab
- [ ] Prevent duplicate tabs
- [ ] Tab persistence across sessions (optional)

**Why:** Consistent UX - everything as tabs, no modals.

---

### 3. Project Settings UI
**Goal:** Allow users to manage project configuration
**Status:** 🟢 Ready to Build
**Details:** See `features/project-settings/todo.md`

**Key Tasks:**
- [ ] Project Settings tab component
- [ ] Edit project name
- [ ] Python package management UI (list, add, remove packages)
- [ ] Export project as ZIP
- [ ] Default project configuration (global settings)
- [ ] Claude Desktop integration ("Add to Claude" button)
- [ ] CLI installation UI and status

**Why:** Users need to manage packages without command line. Backend exists; just needs UI.

---

## Cross-Cutting Improvements

### Workbook Enhancements (Medium Priority)
- [ ] Hover above/below cells to add new cell (code or markdown)
- [ ] Interactive widget support (ipywidgets)
- [ ] Plotly/Bokeh chart support
- [ ] Cell profiling (memory usage, CPU time)

### UX Polish (Ongoing)
- [ ] Loading states for async operations
- [ ] Better error boundaries for React components
- [ ] More keyboard shortcuts (Cmd+W to close tab, Cmd+Tab for tab switching)
- [ ] Improved empty states
- [ ] Tooltips for complex UI elements

### Performance (As Needed)
- [ ] Lazy loading for large file trees (100+ files)
- [ ] Virtual scrolling for long workbook cells (50+ cells)
- [ ] Optimize large DataFrame rendering (10,000+ rows)

---

## Major Features (Post-MVP)

### State Management System
**Goal:** Durable state sharing between workbooks
**Status:** 📋 Design Complete, No Implementation
**Details:** See `features/state/todo.md`

**Key Tasks:**
- [ ] workbooks-core Python package with StateManager class
- [ ] SQLite + blob storage backend
- [ ] state.get() / state.set() API implementation
- [ ] Dependency tracking and graph visualization
- [ ] Checkpointing system for resume functionality
- [ ] StatePanel UI for viewing and managing state
- [ ] (Future) State forking (Neon-style branches)

**Why:** Core differentiating feature for workbook orchestration. This is the biggest feature and will take significant effort.

**Implementation Order:**
1. Python package (workbooks-core) with basic API
2. Backend storage (SQLite + blobs)
3. Dependency tracking system
4. Frontend StatePanel for visualization
5. Checkpointing and resume functionality
6. (Future) State forking and branching

---

## Future Vision (Long-Term)

### Advanced Workbook Features
- [ ] React Flow canvas for visual pipeline connections
- [ ] Variable inspector/debugger integration
- [ ] Workbook templates library
- [ ] Git integration (version control, diffs)
- [ ] Cell folding/collapsing
- [ ] Split view for comparing workbooks

### Platform & Distribution
- [ ] Windows support (currently macOS-focused)
- [ ] Linux support
- [ ] .workbooks file association (double-click to open project)
- [ ] System service/daemon for always-on scheduling
- [ ] Mobile companion app (view-only, check run status)

### Cloud & Collaboration (Optional)
- [ ] Cloud sync and backup for state and runs
- [ ] Team collaboration features (shared state branches)
- [ ] Cloud storage integrations (S3, R2, GCS)

### Integrations
- [ ] Claude Code integration (right-click "Edit with Claude")
- [ ] Database connectors (PostgreSQL, MySQL, DuckDB)
- [ ] External scheduler integration (Airflow compatibility)
- [ ] Webhook triggers for workbook execution

---

## Implementation Guidelines

**When picking up work:**
1. Read `features/<area>/docs.md` to understand the design
2. Check `features/<area>/todo.md` for specific tasks
3. Implement feature
4. Move completed items from `todo.md` to `done.md` (ALWAYS move when done!)
5. Update `features/changelog.md` with completion date and description
6. Test thoroughly before marking complete

**Priority order (December 2025):**
1. **Tier 1 - High Impact** (1-3) - Schedule pending events, Secrets session UI, Workbooks table integration
2. **Tier 2 - Polish & UX** (4-6) - Sidebar enhancements, run reports, state persistence
3. **Tier 3 - Advanced** (7-8) - Auto-detection, tray updates
4. **MVP completion** - Network status, tab navigation, project settings
5. **Major features** - State management system (big lift)
6. **Cross-cutting** - Ongoing improvements as needed
7. **Future vision** - Aspirational, no immediate plans

**Focus:**
- Finish what's started before starting new features
- Prioritize user-facing features over internal refactoring
- Simple > complex - avoid over-engineering
- Local-first - cloud features are optional enhancements
- Update documentation as you go (move todos to done!)

**Current Recommended Focus:**
1. **Schedule: Pending event display & cancellation** (High user value, medium effort)
2. **Secrets: Session lock UI** (High security UX, small effort)
3. **Schedule: Workbooks table integration** (Discoverability, medium effort)
