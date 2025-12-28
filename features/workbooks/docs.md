# Workbooks

## Overview

Workbooks are Jupyter notebooks (`.ipynb` files) with enhanced execution, durability, and state management capabilities. The workbook viewer is the core editing and execution interface.

## WorkbookViewer Component

Located at `src/components/WorkbookViewer.jsx` - This is the main editor for `.ipynb` files.

### Architecture

**Engine Integration:**
- HTTP-based engine server (FastAPI) runs in Python
- Each workbook gets its own Jupyter kernel
- Engines run in project's virtual environment
- Managed via Tauri commands

**Execution Flow:**
1. User executes cell (Shift+Enter, Ctrl/Cmd+Enter, Alt+Enter)
2. Frontend calls `execute_cell_stream()` Tauri command
3. Rust sends HTTP request to engine server
4. Engine server executes code in Jupyter kernel
5. Output streams back via Server-Sent Events (SSE)
6. Frontend renders output in real-time

### Cell Types

**Code Cells:**
- Monaco editor with Python syntax highlighting
- Execute with keyboard shortcuts
- Output area shows results
- Can have multiple output types (text, images, HTML, errors)

**Markdown Cells:**
- Edit mode: Plain text editor
- View mode: Rendered markdown with code syntax highlighting
- Toggle with 'M' key or cell type dropdown

### Output Rendering

Supports rich Jupyter output types:

**Text Outputs:**
- `stdout` - Standard output (print statements)
- `stderr` - Error output (warnings, debug)
- `execute_result` - Return values
- `error` - Exceptions with tracebacks

**Rich Media:**
- `image/png` - PNG images (plots, charts)
- `image/jpeg` - JPEG images
- `image/svg+xml` - SVG graphics
- `text/html` - HTML content (pandas DataFrames, interactive widgets)

**Special Handling:**
- ANSI color codes stripped for clean output
- Large outputs truncated with expand/collapse
- Multiple outputs per cell supported

### Keyboard Shortcuts

**Jupyter-style shortcuts:**
- `Shift+Enter` - Execute cell, select next
- `Ctrl/Cmd+Enter` - Execute cell, stay selected
- `Alt+Enter` - Execute cell, insert new cell below
- `DD` (double-tap D) - Delete selected cell
- `A` - Insert cell above
- `B` - Insert cell below
- `M` - Change to markdown cell
- `Y` - Change to code cell
- `↑/↓` - Navigate between cells
- `Escape` - Exit cell edit mode

### Engine Lifecycle

**States:**
- `starting` - Engine initializing
- `idle` - Ready for execution
- `busy` - Currently running code
- `error` - Engine failed
- `restarting` - Kernel restarting

**Controls:**
- Start engine (automatic on first execution)
- Stop engine (shutdown kernel)
- Interrupt execution (KeyboardInterrupt)
- Restart engine (clear state, fresh kernel)

**Status Indicator:**
- Visual indicator shows current kernel status
- Updates in real-time during execution

### Autosave

**Behavior:**
- Auto-save every 3 seconds when changes detected
- Save on blur (switching tabs/windows)
- Save before execution
- Can be disabled via TabBar toggle

**Tracking:**
- Detects unsaved changes
- Shows indicator in tab (if implemented)

### Cell Operations

**Add/Delete:**
- Insert new cell above/below current
- Delete cell with DD shortcut or button
- Prevent deleting last cell

**Reorder:**
- Move cell up/down with buttons
- Preserves outputs when moving

**Type Change:**
- Convert code ↔ markdown
- Preserves cell content
- Clears outputs when converting to markdown

**Clear Output:**
- Remove all outputs from cell
- Keep code/markdown content
- Useful for cleaning before commit

## File Operations

**Create Workbook:**
- Creates `.ipynb` in `/notebooks` folder
- Initializes with single empty code cell
- Sets label in metadata to the provided name
- Opens in new tab automatically

**Workbook Labels:**
- Each workbook can have a user-friendly label instead of showing the filename
- Label is stored in `notebook.metadata.label`
- Displayed in sidebar, workbooks table view, and tab titles
- Click the workbook title in WorkbookViewer to edit the label
- Press Enter to save, Escape to cancel
- Falls back to filename (without .ipynb) if no label is set
- Makes workbooks feel like actual tools with meaningful names (e.g., "Daily Sales Report" vs "sales_report_v3")

**Save Workbook:**
- Validates notebook structure
- Writes to disk via Tauri command
- Preserves all cell outputs and metadata (including labels)

**Read Workbook:**
- Loads from disk
- Parses JSON structure
- Renders cells and outputs

**Duplicate Workbook:**
- Creates copy with new name
- Preserves all cells and content
- Opens duplicate in new tab

## Integration Points

**With Sidebar:**
- Workbooks section lists all `.ipynb` files
- Click to open in tab
- Recent-use ordering

**With Files:**
- Workbooks stored in `/notebooks` folder
- Excluded from Files section (shown in Workbooks instead)
- Can be renamed/deleted via context menu

**With Engine:**
- One engine per workbook
- Engine lifecycle tied to workbook tab
- WORKBOOKS_PROJECT_FOLDER environment variable injected

**With Secrets (Future):**
- Lock icon when secrets active
- Auto-detection of hardcoded secrets
- Prompt to move to secrets manager
- Output redaction on save
