<!--
Sync Impact Report - Constitution Update
=========================================

VERSION CHANGE: 0.0.0 → 1.0.0
BUMP RATIONALE: Initial constitution creation for Workbooks project (MAJOR)

PRINCIPLES ADDED:
- I. Local-First & Privacy
- II. Minimal UI Design
- III. Feature Documentation Discipline
- IV. Desktop-Native Architecture
- V. Version Synchronization

SECTIONS ADDED:
- Core Principles (5 principles)
- Technical Constraints (Stack & patterns)
- Development Workflow (Feature workflow)
- Governance (Amendment process)

TEMPLATES REQUIRING UPDATES:
✅ plan-template.md - Constitution Check section compatible
✅ spec-template.md - User story format aligns with feature docs workflow
✅ tasks-template.md - Task organization supports feature-based development
⚠ RECOMMENDATION: Add reference to features/ workflow in templates

FOLLOW-UP TODOS:
- Consider adding constitution references to features/README.md
- Evaluate if CLI install/update process needs constitution compliance checks

NOTES:
- Ratification date set to today (2025-12-26) as initial adoption
- Version follows semantic versioning (MAJOR.MINOR.PATCH)
- All placeholders filled based on CLAUDE.md, README.md, and features/ structure
-->

# Workbooks Constitution

## Core Principles

### I. Local-First & Privacy

**Workbooks MUST operate fully offline by default.**

- All execution, state, secrets, and scheduling happens on the user's machine
- Network access is OPTIONAL and ONLY for user-initiated actions (package downloads, cloud sync if configured)
- Users MUST have full control over their data - no telemetry, no cloud requirements, no external dependencies for core functionality
- Secrets MUST be encrypted using system keychains (Touch ID on macOS, equivalent on other platforms)

**Rationale**: Users trust Workbooks with sensitive automation. Privacy isn't a feature; it's the foundation. Desktop-first architecture ensures security, speed, and reliability without cloud dependencies.

### II. Minimal UI Design

**All UI components MUST follow the design patterns in `STYLE_GUIDE.md`.**

- Clean & minimal aesthetic - professional, understated, no visual clutter
- Grayscale + blue accents only - consistent color palette across all components
- No heavy gradients or shadows - flat, modern design
- Tailwind CSS utility classes for all styling - avoid custom CSS unless absolutely necessary
- Consistent spacing using Tailwind's spacing scale
- Semantic colors: blue for primary actions, red for danger, amber for warnings

**Rationale**: A clean, professional UI keeps users focused on their work, not fighting the interface. Consistency across components reduces cognitive load and maintains the app's quality feel.

### III. Feature Documentation Discipline (NON-NEGOTIABLE)

**Every feature MUST be documented in the `features/` directory.**

Each feature area MUST have three files:
- `docs.md` - What the feature is, how it works, design decisions
- `todo.md` - What needs to be implemented
- `done.md` - What has been completed

**Workflow for every feature implementation**:
1. Read `features/<area>/docs.md` to understand the design
2. Check `features/<area>/todo.md` for what needs to be done
3. Check `features/<area>/done.md` to see what's already implemented
4. Implement the feature
5. Move completed items from `todo.md` → `done.md`
6. Add entry to `features/changelog.md` with date and description

**Rationale**: Workbooks is a complex system with many moving parts. Without disciplined documentation, context is lost, duplicate work happens, and the project becomes unmaintainable. This workflow ensures every contributor (human or AI) has the context they need.

### IV. Desktop-Native Architecture

**Workbooks MUST remain a native desktop application built with Tauri.**

Technology stack requirements:
- **Tauri** (Rust + webview) for native desktop app framework
- **React 19 + JSX** (NOT TypeScript) for frontend UI
- **FastAPI + uvicorn** for Python engine server (HTTP-based architecture)
- **Jupyter Client** (AsyncKernelManager) for workbook execution
- **UV** bundled for Python environment and package management
- **SQLite** for local state and metadata storage

**Architecture rules**:
- Frontend communicates via Tauri commands → HTTP → Engine server → Jupyter kernel
- Each workbook gets its own isolated Jupyter engine
- Engines run in the project's venv with custom kernel specs
- Streaming output via event emission from Rust to frontend

**Rationale**: Native desktop performance, system integration (keychains, file system), and bundled Python environments deliver a superior user experience compared to web apps or Electron. The HTTP-based engine architecture provides clean isolation and lifecycle management.

### V. Version Synchronization (NON-NEGOTIABLE)

**Version numbers MUST stay synchronized across all files.**

The project version is defined in three locations:
- `package.json` - npm package version
- `src-tauri/Cargo.toml` - Rust crate version
- `src-tauri/tauri.conf.json` - Tauri app version

**Version bump rules**:
- NEVER manually edit version numbers in individual files
- ALWAYS use `npm run version` script to keep versions synchronized
- Bump version when: completing significant features, creating releases, preparing for production deployment

**Version bump types**:
- **Patch**: `npm run version` (auto-increment) for bug fixes and minor changes
- **Minor**: `npm run version X.Y.0` for new features
- **Major**: `npm run version X.0.0` for breaking changes

**After bumping**: Review with `git diff`, commit with message "Bump version to X.Y.Z", tag with `git tag vX.Y.Z`, push with `git push && git push --tags`

**Rationale**: The CLI version detection system depends on synchronized versions. The app checks installed CLI version and auto-updates if it doesn't match the bundled version. Unsynchronized versions break this critical functionality.

## Technical Constraints

### Technology Stack Requirements

**Language Stack**:
- Rust (latest stable) for Tauri backend
- JavaScript (React 19 + JSX) for frontend - NOT TypeScript
- Python 3.11+ for execution engine
- SQLite for local storage

**Build & Development**:
- Vite for build system with hot reload
- UV bundled for Python environment management
- npm for frontend dependency management
- Tauri CLI for desktop app builds

**UI Components**:
- Monaco Editor for code editing
- React Flow for visual pipeline canvas (installed, planned for future use)
- Tailwind CSS for all styling

**Prohibited Patterns**:
- TypeScript (unless absolutely necessary) - Keep it simple with JSX
- Custom CSS (prefer Tailwind utilities)
- Electron or web-based architectures
- Cloud-required functionality in core features
- Heavy UI frameworks or component libraries

### Performance & Scale Expectations

**Performance Targets**:
- App startup: < 2 seconds on modern hardware
- Workbook execution latency: < 100ms from user action to kernel start
- UI responsiveness: 60 fps for all interactions
- Engine server HTTP response: < 50ms p95 for non-execution commands

**Scale Constraints**:
- Support projects with 100+ workbooks
- Handle workbooks with 1000+ cells
- Manage 10+ concurrent engine processes
- Store 10GB+ of state/checkpoint data per project

## Development Workflow

### Feature Implementation Process

1. **Planning**:
   - Check if feature area exists in `features/`
   - Read existing `docs.md` to understand context
   - Review `todo.md` for planned work
   - Review `done.md` to avoid duplicate work

2. **Implementation**:
   - Follow design decisions in `docs.md`
   - Adhere to UI design patterns in `STYLE_GUIDE.md`
   - Follow architecture rules (Tauri → HTTP → Engine → Jupyter)
   - Keep changes minimal - avoid over-engineering

3. **Completion**:
   - Move completed items from `todo.md` → `done.md`
   - Add entry to `features/changelog.md` with date and description
   - Verify version synchronization if code was changed
   - Test feature in development mode (`npm run tauri dev`)

### Code Review Requirements

**All code changes MUST verify**:
- Feature documentation updated (`todo.md` → `done.md`)
- UI follows `STYLE_GUIDE.md` patterns
- No cloud dependencies introduced for core functionality
- Version numbers remain synchronized if applicable
- No TypeScript introduced unnecessarily
- No custom CSS unless justified

### Complexity Justification

**Complexity MUST be justified when**:
- Adding new external dependencies
- Introducing architectural changes
- Creating abstractions or design patterns
- Deviating from established UI patterns
- Using custom CSS instead of Tailwind

**Justification format**: Document in feature `docs.md` - "Why is this complexity necessary? What simpler alternative was rejected and why?"

## Governance

### Constitution Authority

This constitution supersedes all other development practices, guidelines, and preferences. When conflicts arise between this constitution and other documentation, the constitution takes precedence.

### Amendment Process

**Constitutional amendments require**:
1. **Proposal**: Clear description of change and rationale
2. **Impact Analysis**: Which principles/sections affected, what templates/docs need updates
3. **Version Bump**: MAJOR for principle removal/redefinition, MINOR for additions/expansions, PATCH for clarifications/wording
4. **Sync Update**: Update all dependent templates, docs, and references
5. **Migration Plan**: For breaking changes, document migration path for existing features

**Amendment approval**: Changes to this constitution must be documented in the Sync Impact Report (HTML comment at top of this file) with full traceability.

### Compliance & Review

**All pull requests MUST verify**:
- Feature documentation workflow followed (if feature work)
- UI design patterns adhered to (if UI changes)
- Version synchronization maintained (if version-related changes)
- No violations of Local-First & Privacy principle
- No violations of Desktop-Native Architecture principle

**Periodic constitution review**: Every major version bump, review constitution for relevance and completeness.

### Runtime Development Guidance

For runtime development workflow and AI assistant guidance, see `CLAUDE.md`.

For feature-specific implementation details, see `features/README.md`.

For UI component patterns and styling, see `STYLE_GUIDE.md`.

---

**Version**: 1.0.0 | **Ratified**: 2025-12-26 | **Last Amended**: 2025-12-26
