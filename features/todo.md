# Tether High-Level Roadmap

This file tracks the high-level feature priorities and cross-cutting tasks. For detailed implementation todos, see individual feature directories.

## Immediate Priorities (MVP Completion)

### 1. Network Status & Offline Behavior
**Goal:** Clear user feedback when internet is required
**Details:** See `features/network/todo.md`

**Key Tasks:**
- [ ] Network status indicator (online/offline)
- [ ] Clear error messages when offline
- [ ] Progress indicators for downloads
- [ ] Retry mechanism for failed network operations

**Why:** Users need to know when and why internet is required, especially during first-time setup.

---

### 2. Tab-Based Navigation for Management Views
**Goal:** Replace modals with tabs for consistency
**Details:** See `features/navigation/todo.md`

**Key Tasks:**
- [ ] Add tab types: workbooks-table, secrets, schedule, settings
- [ ] Replace Workbooks table modal with tab
- [ ] Prevent duplicate tabs
- [ ] Tab persistence across sessions

**Why:** Consistent UX - everything as tabs, no modals. Users can keep management views open while working.

---

### 3. Workbooks Section Improvements
**Goal:** Better organization and metadata tracking
**Details:** See `features/sidebar/todo.md` and `features/workbooks/todo.md`

**Key Tasks:**
- [ ] Update ordering: alphabetical within last 5 recent
- [ ] Persist last run times, status, schedule info
- [ ] Make Run and Schedule buttons functional in table view
- [ ] Cell execution status indicators (execution count)

**Why:** Users need to see execution history and quickly identify workbook status.

---

## Near-Term Features

### 4. Project Settings UI
**Goal:** Allow users to manage project configuration
**Details:** See `features/project-settings/todo.md`

**Key Tasks:**
- [ ] Project Settings tab component
- [ ] Edit project name
- [ ] Python package management UI
- [ ] Export project as ZIP

**Why:** Users need to manage packages and configure projects without command line.

---

### 5. Secrets Management
**Goal:** Secure storage for API keys and credentials
**Details:** See `features/secrets/todo.md`

**Key Tasks:**
- [ ] Backend encryption system (keychain integration)
- [ ] Secrets management tab
- [ ] Auto-detection of hardcoded secrets
- [ ] Cell rewriting to use os.environ
- [ ] Output redaction

**Why:** Critical for security and sharing notebooks safely. Prevents accidental credential leaks.

---

## Major Features (Post-MVP)

### 6. Schedule System
**Goal:** Cron-based automation for workbooks
**Details:** See `features/schedule/todo.md`

**Key Tasks:**
- [ ] Scheduler backend with cron support
- [ ] Schedule management tab (Scheduled / Recent Runs)
- [ ] Run tracking and history
- [ ] Run reports storage
- [ ] Background execution while app running

**Why:** Enable automated data pipelines and recurring tasks.

---

### 7. State Management System
**Goal:** Durable state sharing between workbooks
**Details:** See `features/state/todo.md`

**Key Tasks:**
- [ ] tether-core Python package
- [ ] SQLite + blob storage backend
- [ ] state.get() / state.set() API
- [ ] Dependency tracking and graph visualization
- [ ] Checkpointing system

**Why:** Core feature for workbook orchestration. Enables implicit connections and dependency-driven execution.

---

## Cross-Cutting Improvements

### UX Polish
- [ ] Loading states for all async operations
- [ ] Error boundaries for React components
- [ ] Keyboard shortcuts for common operations
- [ ] Better empty states (no workbooks, no files, etc.)
- [ ] Tooltips and help text

### Performance
- [ ] Lazy loading for large file trees
- [ ] Virtual scrolling for long lists
- [ ] Debounce autosave
- [ ] Optimize large output rendering

### Developer Experience
- [ ] Error logging and debugging
- [ ] Performance monitoring
- [ ] User analytics (opt-in)
- [ ] Crash reporting

### Documentation
- [ ] User guide for each feature
- [ ] Keyboard shortcuts reference
- [ ] Troubleshooting guide
- [ ] Video tutorials

### Testing
- [ ] Unit tests for core functionality
- [ ] Integration tests for workflows
- [ ] E2E tests for critical paths
- [ ] Performance benchmarks

## Future Vision (Long-Term)

### Advanced Features
- [ ] React Flow canvas for visual pipeline connections
- [ ] Variable inspector/debugger
- [ ] Cell timing profiler
- [ ] Workbook templates library
- [ ] Git integration
- [ ] Cloud sync and backup
- [ ] Team collaboration features
- [ ] State forking (Neon-style branches)
- [ ] .tether file association (double-click to open)

### Platform Expansion
- [ ] Windows support (currently macOS-focused)
- [ ] Linux support
- [ ] System service/daemon for always-on scheduling
- [ ] Mobile companion app (view-only)

### Integrations
- [ ] Claude Code integration (right-click "Edit with Claude")
- [ ] Database connectors (PostgreSQL, MySQL, etc.)
- [ ] Cloud storage (S3, R2, GCS)
- [ ] External scheduler integration (Airflow, etc.)

## Implementation Guidelines

**When picking up work:**
1. Read `features/<area>/docs.md` to understand the design
2. Check `features/<area>/todo.md` for specific tasks
3. Implement feature
4. Move completed items to `features/<area>/done.md`
5. Update `features/changelog.md` with completion date
6. Test thoroughly before marking complete

**Priority order:**
- MVP completion features (1-4) should be done first
- Major features (5-7) are parallel tracks, can be started anytime
- Cross-cutting improvements are ongoing
- Future vision items are aspirational

**Focus:**
- Finish what's started before starting new features
- Prioritize user-facing features over internal refactoring
- Simple > complex - avoid over-engineering
- Local-first - cloud features are optional enhancements
