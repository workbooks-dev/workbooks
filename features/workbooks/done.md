# Workbooks - Completed

## WorkbookViewer Component (~85% MVP Complete)

### Core Editor

- [x] Monaco editor for code cells
  - [x] Python syntax highlighting
  - [x] Code completion
  - [x] Multi-line editing

- [x] Markdown cell support
  - [x] Edit mode with plain text editor
  - [x] View mode with rendered markdown
  - [x] Code syntax highlighting in markdown (via react-syntax-highlighter)
  - [x] Toggle between edit/view modes
  - [x] Enhanced markdown rendering with plugins:
    - [x] remark-gfm (GitHub Flavored Markdown) - tables, strikethrough, task lists
    - [x] remark-math - mathematical expressions
    - [x] rehype-katex - LaTeX math rendering
    - [x] rehype-raw - HTML support in markdown
  - [x] Rich markdown features:
    - [x] Bold, italic, strikethrough text
    - [x] Headers (h1-h6) with custom styling
    - [x] Ordered and unordered lists
    - [x] Task lists with checkboxes
    - [x] Tables with custom styling and hover effects
    - [x] Code blocks with syntax highlighting
    - [x] Inline code with background styling
    - [x] Blockquotes with left border styling
    - [x] Links (external URLs open in new tab)
    - [x] Math equations using LaTeX syntax ($...$ and $$...$$)
  - [x] Image support:
    - [x] Remote images from URLs (http/https)
    - [x] Local images via relative paths (e.g., `./images/plot.png`)
    - [x] Local images via absolute paths
    - [x] Environment variable substitution ($WORKBOOKS_PROJECT_FOLDER and ${WORKBOOKS_PROJECT_FOLDER})
    - [x] Automatic conversion to Tauri asset protocol
    - [x] Error handling with fallback image
    - [x] Responsive image sizing with rounded corners
  - [x] Link handling:
    - [x] External links open in new tab with security attributes
    - [x] Local file link detection (ready for future file viewer integration)
    - [x] Custom styling for different link types
  - [x] Custom CSS styling using Tailwind Typography (prose classes)
  - [x] Full markdown persistence to .ipynb files

### Execution System

- [x] Cell execution with keyboard shortcuts
  - [x] Shift+Enter (execute, select next)
  - [x] Ctrl/Cmd+Enter (execute, stay)
  - [x] Alt+Enter (execute, insert below)

- [x] Code preprocessing before execution
  - [x] Automatic `!cd` to `%cd` conversion (makes directory changes persist across cells)

- [x] Streaming output via Server-Sent Events
  - [x] Real-time stdout/stderr
  - [x] Progressive output rendering
  - [x] Handles long-running cells

- [x] Rich output rendering
  - [x] PNG images (with click-to-zoom lightbox)
  - [x] JPEG images (with click-to-zoom lightbox)
  - [x] SVG graphics
  - [x] HTML content (DataFrames, tables)
  - [x] Error tracebacks
  - [x] Text output (stdout, stderr, execute_result)

- [x] Execution metadata tracking
  - [x] Last run timestamp stored in cell metadata
  - [x] Execution duration per cell
  - [x] Metadata displayed below execution count
  - [x] Persisted in notebook file

- [x] Cell execution status indicators
  - [x] Execution count display `[3]` like Jupyter
  - [x] Running indicator (blue text, live timer)
  - [x] Error indicator (✗ symbol, red highlighting)
  - [x] Execution duration shown after completion

- [x] Execution queue controls
  - [x] "Run All" button to execute all cells sequentially
  - [x] "Run All Above" to execute cells above selected
  - [x] "Run All Below" to execute cells below selected
  - [x] Queue progress tracking with cell highlighting

### Engine Management

- [x] Engine lifecycle controls
  - [x] Start engine (automatic on first run)
  - [x] Stop engine
  - [x] Interrupt execution (Ctrl+C equivalent)
  - [x] Restart engine (clear kernel state)
  - [x] Force restart engine (kills kernel process if stuck)
  - [x] Cleanup orphaned kernels (kills all orphaned Jupyter processes)

- [x] Kernel status indicator
  - [x] Real-time status display
  - [x] States: starting, idle, busy, error, restarting
  - [x] Visual feedback during execution

- [x] Stuck kernel recovery
  - [x] Force Restart button always available (highlighted when engine in error state)
  - [x] Kills kernel process with SIGKILL if graceful shutdown fails
  - [x] Backend endpoint to cleanup all orphaned Jupyter kernels
  - [x] No need to restart entire app when kernel gets stuck

### Jupyter Keyboard Shortcuts

- [x] DD (double-tap) - Delete cell
- [x] A - Insert cell above
- [x] B - Insert cell below
- [x] M - Change to markdown cell
- [x] Y - Change to code cell
- [x] Up/Down arrows - Navigate cells
- [x] Escape - Exit cell edit mode

### Cell Operations

- [x] Add new cells (above/below)
- [x] Delete cells
- [x] Move cells up/down (with proper React key handling for UI updates)
- [x] Change cell type (code/markdown)
- [x] Clear cell outputs
- [x] Prevent deleting last cell

### File Operations

- [x] Create new workbook
  - [x] Create blank workbook
  - [x] Generate with AI from description
    - [x] Uses Claude CLI to generate notebook cells from user description
    - [x] Automatically creates markdown and code cells with appropriate content
    - [x] Fallback to blank workbook with error dialog if generation fails
    - [x] Validates and parses AI response to ensure valid .ipynb structure
- [x] Save workbook (manual and auto)
- [x] Load workbook from disk
- [x] Duplicate workbook
- [x] Rename workbook (via context menu)
- [x] Delete workbook (via context menu)

### Autosave System

- [x] Auto-save every 3 seconds when dirty
- [x] Save on blur (tab/window switch)
- [x] Save before cell execution
- [x] Toggle autosave on/off
- [x] Unsaved changes tracking

### Output Handling

- [x] ANSI color code stripping
- [x] Output truncation for large outputs
- [x] Expand/collapse for truncated output
- [x] Multiple outputs per cell
- [x] Clear formatting
- [x] Enhanced DataFrame rendering
  - [x] Sticky headers for scrollable tables
  - [x] Max height with scroll for large DataFrames
  - [x] Improved border styling and spacing
  - [x] Zebra striping with hover effects
  - [x] Gradient headers with subtle shadows
  - [x] Tabular numeric formatting
- [x] Image lightbox/zoom
  - [x] Click to zoom on PNG images
  - [x] Click to zoom on JPEG images
  - [x] Dark overlay with centered image
  - [x] Close button and click-outside-to-close
  - [x] Hover effects on thumbnails

### Integration

- [x] Environment variable injection
  - [x] WORKBOOKS_PROJECT_FOLDER available in kernel
- [x] Project venv integration
  - [x] Runs in isolated virtual environment
  - [x] Access to project packages

### Workbook Labels

- [x] User-friendly labels instead of filenames
  - [x] Labels stored in notebook metadata under `metadata.label`
  - [x] Displayed in sidebar, workbooks table view, and tab titles
  - [x] Click-to-edit UI in WorkbookViewer header
  - [x] Fallback to filename when no label is set
  - [x] Automatically set on new workbook creation
  - [x] Enter to save, Escape to cancel editing
  - [x] Makes workbooks feel like actual tools with meaningful names

### UI/UX Polish

- [x] Cell visual improvements
  - [x] Clear cell borders with rounded corners
  - [x] Improved execution indicator styling [1], [ ], etc
  - [x] Subtle cell hover states
  - [x] Visual separators between cells
  - [x] Matches STYLE_GUIDE.md grayscale + blue aesthetic

- [x] Output area improvements
  - [x] Better DataFrame styling (borders, zebra striping, header styling)
  - [x] Padding/margins on output containers
  - [x] Improved plain text output styling
  - [x] Better visual distinction between code and output areas
  - [x] Subtle background colors on output areas

- [x] Toolbar refinements
  - [x] Better button spacing and grouping (Execution / Kernel / Add Cells)
  - [x] Icons added to buttons (▶, ⏹, 🔄, 🗙)
  - [x] Consistent button styling with rest of app
  - [x] Visual hierarchy with separators

- [x] Monaco editor styling
  - [x] Improved line number appearance
  - [x] Adjusted editor padding (top/bottom 8px)
  - [x] Cleaner cell borders around editor
  - [x] Removed unnecessary UI elements (glyph margin, folding)
