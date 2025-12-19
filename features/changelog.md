# Tether Development Changelog

This file tracks major features and improvements as they're completed.

## Recent Completions

### December 2024

**Workbook Execution Enhancements (Dec 18, 2024)**
- **Execution Metadata Tracking**: Cell-level performance metrics
  - Last run timestamp stored in cell metadata
  - Execution duration tracked and displayed
  - Duration shown below execution count ([3] 0.25s)
  - Metadata persisted in notebook file for history
- **Cell Execution Status Indicators**: Visual feedback system
  - Error indicator (✗ symbol) with red highlighting on failed cells
  - Running indicator with blue text and live timer
  - Execution count display matching Jupyter style [3]
  - Duration displayed after completion
- **Execution Queue Controls**: Batch cell execution
  - "Run All Above" button to execute cells above selected
  - "Run All Below" button to execute cells below selected
  - Enhanced "Run All" with metadata tracking
  - Queue progress tracking with cell highlighting
- **Enhanced DataFrame Rendering**: Production-grade table styling
  - Sticky headers that stay visible when scrolling
  - Max height (600px) with scroll for large DataFrames
  - Cleaner borders (bottom-only instead of full grid)
  - Gradient headers with subtle shadows
  - Improved hover effects with smooth transitions
  - Tabular numeric formatting for better number alignment
  - Sticky left column for row indices
- **Image Lightbox/Zoom Feature**: Click-to-zoom functionality
  - Click any PNG/JPEG image to view full-size
  - Dark overlay with centered image
  - Close button and click-outside-to-close behavior
  - Hover effects on thumbnails (cursor change, opacity)
  - Supports images up to 90vh height

**Workbook UI Polish (Dec 18, 2024)**
- **Cell Visual Improvements**: Complete redesign of cell appearance
  - Added clear borders with rounded corners and hover states
  - Improved selection states with blue borders and subtle backgrounds
  - Better execution indicator styling [1], [ ]
  - Tighter spacing between cells for better organization
- **DataFrame Output Styling**: Professional table rendering
  - Zebra striping (alternating row colors)
  - Bold headers with gray backgrounds
  - Proper borders on all cells
  - Hover effects on rows (blue highlight)
  - Better spacing and typography
- **Output Area Enhancements**: Improved all output types
  - Stream outputs with rounded borders and subtle backgrounds
  - Error outputs with red tint for visibility
  - Images with white backgrounds and padding
  - Plain text with proper monospace styling
- **Toolbar Refinements**: Better organization and visual hierarchy
  - Logical grouping (Execution / Kernel / Add Cells)
  - Icons added to all buttons (▶, ⏹, 🔄, 🗙)
  - Visual separators between groups
  - Improved spacing and button styling
- **Monaco Editor Polish**: Cleaner code editing experience
  - Added vertical padding (8px top/bottom)
  - Border around editor container
  - Removed unnecessary UI elements (glyph margin, folding)
  - Consistent with STYLE_GUIDE.md aesthetic

**File Management Feature Enhancements (Dec 18, 2024)**
- **Image Viewer**: Added full image viewing support for PNG, JPG, SVG, GIF, WebP, BMP, ICO
  - Zoom controls (25% - 400%)
  - Reset zoom button
  - Clean, centered display with controls
- **CSV Preview**: Implemented interactive table viewer for CSV files
  - Sortable columns (click header to sort ascending/descending)
  - Automatic numeric vs string detection for sorting
  - Row and column count display
  - Toggle between table view and raw CSV editor
  - Performance optimized (displays first 1000 rows)
- **JSON Tree Viewer**: Built collapsible tree structure for JSON files
  - Expandable/collapsible nodes
  - Type-based syntax highlighting (strings, numbers, booleans, null)
  - Shows object/array size previews when collapsed
  - Auto-expands first 2 levels
  - Toggle between tree view and raw JSON editor
- **File Search**: Added real-time search/filter in Files section
  - Search by filename
  - Live filtering as you type
  - Clear "no matches" messaging
- **File Creation**: Implemented create new file and folder functionality
  - "+ File" and "+ Folder" buttons in Files section
  - Inline creation forms with validation
  - Auto-refresh file list after creation
  - Backend Tauri commands: `create_new_file()`, `create_new_folder()`
- **Visual Drop Zone**: Confirmed existing drop zone indicator working
  - Blue dashed border overlay when dragging files
  - Clear messaging about file destinations
- Updated features/files documentation (done.md, todo.md) with completed items

**Secrets Output Warning System (Dec 18, 2024)**
- Implemented proactive warning system to prevent secret leakage in workbook outputs
- Added `scan_outputs_for_secrets` Tauri command to detect secrets in cell outputs
- **Proactive detection**: Automatically scans outputs after every cell execution
  - Save button changes to amber "⚠ Save" when secrets detected
  - Tooltip warns: "Secrets detected in outputs - click to review"
  - Visual feedback BEFORE user attempts to save
- Created SecretsWarningModal component with professional, clean design
  - Warning icon and clear messaging about security risks
  - Shows list of affected cell indices (e.g., "Cell [1]", "Cell [3]")
  - Three action options: "Clear and Save", "Go Back and Fix", "Dangerously Save Anyway"
  - Two-step confirmation for dangerous save action
  - Follows app style guide (amber warning colors, proper typography)
- Integrated scanning into WorkbookViewer workflow
  - Scans on cell execution, not just on save (proactive vs reactive)
  - Blocks save if secrets detected until user makes a choice
  - Prevents accidental exposure of secrets in Git commits or shared notebooks
- Backend scanning logic checks all cell outputs against stored secret values
- Updated secrets documentation (todo.md, done.md) to reflect completion

**UI Style Guide & Secrets Manager Redesign (Dec 18, 2024)**
- Created comprehensive `STYLE_GUIDE.md` defining Tether's design system
- Redesigned Touch ID authentication gate to match app aesthetic
  - Removed heavy gradients and shadows
  - Changed from purple gradient to clean gray background
  - Replaced gradient button with standard blue primary button
  - Centered layout with proper spacing and typography
  - Fixed authentication gate header to use Tailwind (removed old CSS classes)
- Complete redesign of AddSecretDialog component
  - Converted from inline CSS to Tailwind utility classes
  - Removed emoji buttons (🔐, 👁️) replaced with text ("Show", "Hide", "Authenticate")
  - Changed yellow warning background to clean blue info box
  - Dialog now follows style guide overlay and card pattern
  - Improved form inputs with proper focus states
- Secrets Manager main interface redesign
  - Removed all inline `<style>` block (280+ lines of custom CSS)
  - Converted entire component to Tailwind utilities
  - Table action buttons changed from emoji (✏️, 🗑️) to text ("Edit", "Delete")
  - Consistent button styling (primary, secondary, danger patterns)
  - Improved table styling with proper hover states
  - Clean, professional aesthetic throughout
  - Removed emoji from header titles (both auth and main view)
- Secrets tab improvements
  - Changed tab name from "🔐 Secrets" to just "Secrets" (removed emoji)
  - Clean, professional appearance in tab bar
- Sidebar emoji removal (complete cleanup)
  - Removed emojis from all section headers (Workbooks, Secrets, Schedule, Files)
  - Removed file type emojis (🐍 for .py, 📝 for .md, ⚙️ for config files, etc.)
  - Removed workbook list item emoji (📓)
  - Removed Project Settings button emoji (⚙️)
  - Kept functional arrows (▶/▼) for folder expand/collapse
  - Clean, text-only interface throughout sidebar
- Tab bar cleanup
  - Removed all file type emojis from tabs (📓, 🐍, 📝, ⚙️, 📄)
  - Removed autosave toggle (non-functional UI element)
  - Tab bar now shows only when tabs are open
  - Clean, minimal tab display with just filenames
- WorkbookViewer toolbar cleanup
  - Removed secrets count badge (🔐 with count)
  - Removed Admin Mode toggle button (🔒/🔓)
  - Removed all secrets-related state and functions (loadSecretsCount, toggleAdminMode)
  - Secrets in cell output remain redacted (security feature preserved)
  - Cleaner, less cluttered toolbar
- Updated CLAUDE.md with UI Design section referencing style guide
- Style guide includes: color palette, typography, spacing, component patterns, layouts, accessibility
- All future UI work must follow the approved design patterns
- Files: `STYLE_GUIDE.md` (new), `src/components/SecretsManager.jsx` (redesigned), `src/components/Sidebar.jsx` (cleaned), `src/components/TabBar.jsx` (cleaned), `src/components/WorkbookViewer.jsx` (cleaned), `src/App.jsx` (updated), `CLAUDE.md` (updated)

**Secrets Management (Dec 18, 2024)**
- Complete secrets management system with encryption, keychain integration, and UI
- AES-256-GCM encryption with per-project keys stored in system keychain
- SQLite database for encrypted secrets storage (`.tether/secrets.db`)
- Full CRUD interface via SecretsManager component
- Automatic injection of secrets as environment variables into workbook kernels
- Sidebar integration with live count and quick access
- WorkbookViewer indicator badge showing active secrets
- Import from .env files functionality
- 7 new Tauri commands: add_secret, get_secret, list_secrets, update_secret, delete_secret, get_all_secrets, import_secrets_from_env
- Real-time updates via event system
- Files: `src-tauri/src/secrets.rs` (new), `src/components/SecretsManager.jsx` (new), updated engine_http.rs, lib.rs, Sidebar.jsx, WorkbookViewer.jsx, App.jsx
- See `features/secrets/done.md` for full details

**Workbook Execution System (~85% MVP Complete)**
- Full-featured WorkbookViewer with Monaco editor
- Streaming output via Server-Sent Events for real-time feedback
- Rich output rendering (PNG, JPEG, SVG, HTML, DataFrames)
- Engine lifecycle management (start, stop, interrupt, restart)
- Kernel status indicator with real-time updates
- Jupyter-style keyboard shortcuts (DD delete, A/B insert, M/Y type change, arrows)
- Auto-save system (3s interval + on-blur + on-run) with toggle
- Cell operations (add, delete, move, type change, clear output)

**File Management**
- Complete file operations backend (read, write, rename, delete, duplicate)
- FileViewer with Monaco editor and multi-language support
- Markdown preview mode with rendered output
- Context menu for file operations (rename, delete, duplicate)
- Input dialog for rename/duplicate flows
- Drag-and-drop file upload (.ipynb → /notebooks, others → root)
- TETHER_PROJECT_FOLDER environment variable injection into kernels

**Navigation & UI**
- Tab-based navigation system for multiple open files
- TabBar component with autosave toggle
- Support for workbook and file tabs
- Active tab highlighting and close functionality

**Sidebar**
- Multi-section sidebar layout (Workbooks, Secrets, Schedule, Files, Settings)
- Workbooks section with recent-use ordering (last 20 tracked)
- Workbooks table view modal with metadata
- Files section with tree view and file type icons
- Secrets and Schedule section placeholders
- Project Settings gear icon

**Project & Python Management**
- Project creation with uv integration
- Virtual environment management (centralized at ~/.tether/venvs/)
- Python package installation via uv
- Dependency syncing from pyproject.toml
- HTTP engine server (FastAPI) for Jupyter kernel management
- Per-workbook engine isolation

**Backend Infrastructure**
- Tauri app scaffolding (Rust + React 19)
- Python/uv integration with automatic installation
- File system operations (list, read, write, rename, delete)
- Engine HTTP server for kernel lifecycle
- Project management (create, open, load)

### Earlier Work

**Initial Setup**
- Tauri + React 19 + Vite build system
- Welcome screen and create project wizard
- Basic application state management
- File explorer with collapsible tree view

## What's Next

See `features/todo.md` for the high-level roadmap and individual feature areas for detailed implementation plans.

**Priority Features:**
1. Network status indicators and offline behavior (see features/network/)
2. Tab-based navigation for management views (see features/navigation/)
3. Secrets management system (see features/secrets/)
4. Schedule system with cron scheduling (see features/schedule/)
5. Project settings UI (see features/project-settings/)
6. State management system (see features/state/) - Major future feature
