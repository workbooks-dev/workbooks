# Secondary Benefits

Supporting features that enhance the core experience.

## Kernel Control

Full control over the Jupyter kernel lifecycle: interrupt long-running cells, restart the kernel to clear state, or reconnect if the engine crashes.

**Implementation:** `WorkbookViewer.jsx:1094-1105` interrupts via HTTP; `WorkbookViewer.jsx:1053-1092` restarts kernel; `engine_server.py:671-683` and `686-700` handle interrupt/restart.

## Jupyter-Compatible Keyboard Shortcuts

Standard Jupyter keyboard shortcuts work as expected:
- **Shift+Enter** - Run cell and move to next
- **Ctrl/Cmd+Enter** - Run cell and stay
- **Alt+Enter** - Run cell and insert below
- **A** / **B** - Insert cell above/below (command mode)
- **DD** - Delete cell (double-tap D)
- **M** / **Y** - Change to markdown/code
- **Cmd+S** - Save notebook

**Implementation:** `WorkbookViewer.jsx:928-998` handles global shortcuts; `WorkbookCell:186-197` handles cell-specific execution shortcuts.

## File Browser with Context Menu

Integrated file tree in the sidebar lets you browse, rename, delete, and create files and folders. Right-click context menu provides quick access to file operations and "Reveal in Finder".

**Implementation:** `Sidebar.jsx:42-150` renders file tree; `ContextMenu.jsx` provides right-click actions.

## Organized Sidebar Structure

Sidebar is divided into logical sections:
- **Workbooks** - Quick access to .ipynb files with table view
- **Secrets** - Manage encrypted secrets
- **Schedule** - (Placeholder for future scheduling)
- **Files** - Browse all project files

**Implementation:** `Sidebar.jsx` provides collapsible sections; `WorkbooksTableView.jsx` shows workbook list with metadata.

## Secret Detection Before Save

Workbooks scans cell outputs for secret values before saving. If secrets are detected, a modal warns you with options to:
- Clear outputs from affected cells and save
- Go back and manually clear
- Force save anyway (discouraged)

**Implementation:** `WorkbookViewer.jsx:903-924` scans outputs; `SecretsWarningModal.jsx` provides UI; Rust `scan_outputs_for_secrets` command checks each cell.

## Standard Notebook Format

Workbooks saves notebooks as standard `.ipynb` files. Edit them in Jupyter, VS Code, or any other tool. Workbooks just adds metadata for execution timing and doesn't break compatibility.

**Implementation:** `lib.rs:read_workbook` and `save_workbook` commands read/write standard Jupyter format; cell metadata includes optional `workbooks.duration_ms` and `workbooks.last_run`.

## Markdown Cell Rendering

Markdown cells render with full GitHub Flavored Markdown support, including:
- Tables
- Syntax-highlighted code blocks
- Math equations (KaTeX)
- Images (local and remote)
- Links

**Implementation:** `WorkbookViewer.jsx:256-382` uses ReactMarkdown with remark-gfm, remark-math, rehype-katex, and syntax-highlighter.

## Rich Output Display

Cells display rich Jupyter outputs:
- Images (PNG, JPEG, SVG)
- HTML (pandas DataFrames, matplotlib plots)
- Text output with ANSI color code stripping
- Error tracebacks with formatting

**Implementation:** `WorkbookViewer.jsx:611-842` handles all Jupyter MIME types; output priority: images → HTML → text/plain → raw JSON.

## Output Truncation

Long outputs are automatically truncated to prevent UI slowdown. Shows first 100 messages with option to expand. Backend limits output to prevent memory issues.

**Implementation:** `engine_server.py:310-314` sets max output limits; `WorkbookViewer.jsx:612-653` provides expand/collapse UI.

## Engine Status Indicator

Header shows real-time engine status:
- ⏳ Starting - Engine is initializing
- ● Idle - Ready to execute
- ⚡ Busy - Running a cell
- 🔄 Restarting - Kernel restart in progress
- ⚠ Error - Connection lost

**Implementation:** `WorkbookViewer.jsx:855` tracks status; `WorkbookViewer.jsx:1766-1803` renders status badge with color coding.

## Execution Timing

Each cell shows execution time during and after running. Live timer updates every 100ms while running. After completion, shows total duration from metadata.

**Implementation:** `WorkbookViewer.jsx:1421-1431` starts timer on execution; `WorkbookViewer.jsx:449-458` displays elapsed/completed time.
