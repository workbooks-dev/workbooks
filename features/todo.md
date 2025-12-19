# Tether High-Level Roadmap

This file tracks the high-level feature priorities and cross-cutting tasks. For detailed implementation todos, see individual feature directories.

## Current Status Summary (Updated Dec 19, 2024)

### ✅ Completed Core Features
- **Workbooks System** (~85% MVP complete) - Full execution, streaming output, rich rendering, keyboard shortcuts
- **Files Management** - Complete CRUD operations, image/CSV/JSON viewers, drag-and-drop, context menus
- **Secrets Management** - Full encryption, CRUD UI, kernel injection, output warning system, Touch ID auth
- **Navigation** - Tab system, native macOS menu bar, multi-file support
- **Sidebar** - Multi-section layout (Workbooks, Secrets, Schedule, Files, Settings)
- **Python/uv Integration** - Virtual environments, package management, centralized venv storage
- **UI System** - Complete style guide, professional grayscale + blue aesthetic, no emojis

### 🔴 Critical Issues (Fix First)

✅ **All critical issues resolved!** (Completed Dec 19, 2024)

### 🟡 High-Priority Gaps (MVP Completion)
1. Network status indicators and error handling
2. Tab-based navigation for management views (workbooks table, settings)
3. Project Settings UI (backend exists, needs frontend)

### 📋 Major Features (Post-MVP, Fully Designed)
1. Schedule System - Cron scheduling, run history, automation
2. State Management - Shared state, dependency tracking, orchestration

---

## Immediate Priorities (MVP Completion)

### ~~1. Critical Workbook Bugs~~ ✅ COMPLETED
**Status:** ✅ Completed Dec 19, 2024

**Completed Tasks:**
- ✅ Fixed cell movement UI bug (now uses stable React keys for proper re-rendering)
- ✅ Fixed markdown image display (supports `$TETHER_PROJECT_FOLDER` and `${TETHER_PROJECT_FOLDER}` syntax)

---

### ~~2. Files Section Subfolder Support~~ ✅ COMPLETED
**Status:** ✅ Completed Dec 19, 2024

**Completed Tasks:**
- ✅ Subfolder tree view with expand/collapse
- ✅ Recursive file search through all subfolders
- ✅ File path display in search results
- ✅ Debounced search for better performance

**Remaining:**
- Drag files into folders/subfolders (lower priority)
- Fix "+ New Folder" focus retention issue (major UX issue)

---

### 1. Network Status & Offline Behavior
**Goal:** Clear user feedback when internet is required
**Status:** 🟡 Medium-High Priority
**Details:** See `features/network/todo.md`

**Key Tasks:**
- [ ] Network status indicator (online/offline)
- [ ] Clear error messages when offline
- [ ] Progress indicators for downloads (uv, packages)
- [ ] Retry mechanism for failed network operations

**Why:** Backend network operations exist but have no UI feedback. Users are confused during first-time setup when downloads happen silently or fail.

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

**Why:** Consistent UX - everything as tabs, no modals. Secrets already uses tabs; workbooks table still uses modal.

**Note:** Secrets and Schedule sections already open as tabs, so this is primarily about workbooks-table and settings.

---

## Near-Term Features

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

**Why:** Users need to manage packages and configure projects without command line. Backend foundation exists; just needs UI.

---

### 4. Workbooks Section Polish
**Goal:** Better organization and metadata in workbooks list
**Status:** 🟢 Enhancement
**Details:** See `features/sidebar/todo.md` and `features/workbooks/todo.md`

**Key Tasks:**
- [ ] Update ordering: last 5 recent (alphabetically sorted) + remaining alphabetically
- [ ] Persist last run times in workbooks table (currently shows "Never")
- [ ] Persist run status (currently shows "Not Run")
- [ ] Persist schedule info (currently shows "No")
- [ ] Make Run button functional in table view
- [ ] Make Schedule button functional in table view

**Why:** Improve UX for tracking workbook execution history. Core functionality works; this is polish.

---

### 5. Secrets Enhancements
**Goal:** Complete the secrets system with advanced features
**Status:** ✅ Core Complete, Enhancements Remaining
**Details:** See `features/secrets/todo.md`

**Completed:**
- ✅ AES-256-GCM encryption with keychain integration
- ✅ Full CRUD UI in SecretsManager tab
- ✅ Automatic injection into workbook kernels
- ✅ Proactive secrets detection in outputs with warning modal
- ✅ Touch ID authentication with session management

**Remaining Enhancements:**
- [ ] Backend automatic redaction (complement to frontend warning)
- [ ] Auto-detection of hardcoded secrets in cells
- [ ] Cell rewriting to use `os.environ`
- [ ] Visual lock/unlock indicator
- [ ] Session expiry on app close

**Why:** Core security is complete and working. Remaining items are nice-to-have enhancements.

---

## Major Features (Post-MVP)

### 6. Schedule System
**Goal:** Cron-based automation for workbooks
**Status:** 📋 Design Complete, No Implementation
**Details:** See `features/schedule/todo.md`

**Key Tasks:**
- [ ] Scheduler backend with cron parsing and execution
- [ ] Schedule management tab (Scheduled Workbooks / Recent Runs)
- [ ] Run tracking database and history
- [ ] Run reports storage (.tether/runs/)
- [ ] Background task runner (tokio)
- [ ] Sidebar integration (next run display)
- [ ] Workbooks table "Schedule" button

**Why:** Enable automated data pipelines and recurring tasks. Fully designed but awaiting implementation.

**Implementation Order:**
1. Backend scheduler system and run tracking
2. Schedule management tab UI
3. Integration with workbooks table and sidebar
4. (Future) System service for always-on scheduling

---

### 7. State Management System
**Goal:** Durable state sharing between workbooks
**Status:** 📋 Design Complete, No Implementation
**Details:** See `features/state/todo.md`

**Key Tasks:**
- [ ] tether-core Python package with StateManager class
- [ ] SQLite + blob storage backend
- [ ] state.get() / state.set() API implementation
- [ ] Dependency tracking and graph visualization
- [ ] Checkpointing system for resume functionality
- [ ] StatePanel UI for viewing and managing state
- [ ] (Future) State forking (Neon-style branches)

**Why:** Core differentiating feature for workbook orchestration. Enables implicit connections and dependency-driven execution. This is the biggest feature and will take significant effort.

**Implementation Order:**
1. Python package (tether-core) with basic API
2. Backend storage (SQLite + blobs)
3. Dependency tracking system
4. Frontend StatePanel for visualization
5. Checkpointing and resume functionality
6. (Future) State forking and branching

---

## Cross-Cutting Improvements

### UX Polish (Ongoing)
- [ ] Loading states for async operations (package installs, file uploads, etc.)
- [ ] Better error boundaries for React components
- [ ] More keyboard shortcuts (Cmd+W to close tab, Cmd+Tab for tab switching)
- [ ] Improved empty states (no workbooks, no secrets, no schedules)
- [ ] Tooltips for complex UI elements
- [ ] Confirmation dialogs for destructive actions

### Performance (As Needed)
- [ ] Lazy loading for large file trees (100+ files)
- [ ] Virtual scrolling for long workbook cells (50+ cells)
- [ ] Optimize large DataFrame rendering (10,000+ rows)
- [ ] Debounce file search input

### Workbook Enhancements (Medium Priority)
- [ ] Execution state persistence on tab changes (currently reverts to saved state)
- [ ] Hover above/below cells to add new cell (code or markdown)
- [ ] Interactive widget support (ipywidgets)
- [ ] Plotly/Bokeh chart support
- [ ] Cell profiling (memory usage, CPU time)

### File Management Enhancements (Low Priority)
- [ ] SQLite database browser
- [ ] Parquet file preview
- [ ] Excel file viewer
- [ ] PDF viewer
- [ ] Image metadata display (dimensions, file size)
- [ ] CSV filtering and editing

### Testing (Future)
- [ ] Automated tests for critical paths
- [ ] Workbook execution tests
- [ ] File operations tests
- [ ] Secrets encryption tests

## Future Vision (Long-Term)

### Advanced Workbook Features
- [ ] React Flow canvas for visual pipeline connections (dependency graph)
- [ ] Variable inspector/debugger integration
- [ ] Workbook templates library
- [ ] Git integration (version control, diffs)
- [ ] Cell folding/collapsing
- [ ] Split view for comparing workbooks

### Platform & Distribution
- [ ] Windows support (currently macOS-focused)
- [ ] Linux support
- [ ] .tether file association (double-click to open project)
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

## Implementation Guidelines

**When picking up work:**
1. Read `features/<area>/docs.md` to understand the design
2. Check `features/<area>/todo.md` for specific tasks
3. Implement feature
4. Move completed items from `todo.md` to `done.md` (ALWAYS move when done!)
5. Update `features/changelog.md` with completion date and description
6. Test thoroughly before marking complete

**Priority order (December 2024):**
1. ~~**Critical bugs**~~ ✅ COMPLETED (Dec 19, 2024)
2. **MVP completion** (1-2) - Essential for v1.0 release
3. **Near-term features** (3-5) - Polish and enhancements, nice-to-have
4. **Major features** (6-7) - Post-MVP, big lifts with full designs ready
5. **Cross-cutting** - Ongoing improvements as needed
6. **Future vision** - Aspirational, no immediate plans

**Focus:**
- Fix bugs before adding features
- Finish what's started before starting new features
- Prioritize user-facing features over internal refactoring
- Simple > complex - avoid over-engineering
- Local-first - cloud features are optional enhancements
- Update documentation as you go (move todos to done!)

**Current Focus Areas:**
- ~~Workbook bugs (cell movement, markdown images)~~ ✅ COMPLETED
- ~~Files subfolder support~~ ✅ COMPLETED
- Network status and error handling
- Tab-based navigation for management views
- Project settings UI
