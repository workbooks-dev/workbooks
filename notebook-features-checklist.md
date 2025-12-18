## Jupyter Notebook Feature Checklist

**Current Status: 85% MVP Complete** (Last updated: 2025-12-18)

The notebook viewer has excellent core functionality - editing, execution, keyboard shortcuts, rich output rendering, streaming output, interrupt, kernel status, and file management all work well. The main gaps are drag & drop cell reordering, hover-between-cells insert UI, and undo/redo for structural changes.

### Recent Updates (2025-12-18)

Upon review of the actual implementation, discovered that several features previously marked as missing are actually **fully implemented**:

- ✅ **Rich Output Rendering** - PNG, JPEG, SVG, HTML all working (WorkbookViewer.jsx:301-411)
- ✅ **Interrupt Execution** - Full backend + frontend implementation with toolbar button
- ✅ **Kernel Status Indicator** - Real-time status display (starting/idle/busy/error/restarting)
- ✅ **DD Double-Tap Delete** - Proper Jupyter-style deletion with 500ms window
- ✅ **File Operations** - Rename, delete, duplicate all working with context menu

These improvements raise the completion from 70% → **85% MVP Complete**.

---

## Priority Action Items

To reach full MVP parity with Jupyter, implement these in order:

### P0 - Critical for Basic Usability
1. ~~**Rich output rendering** - Images, HTML, tables, plots~~ ✅ **COMPLETED**
   - ✅ Implemented in CellOutput component (WorkbookViewer.jsx:301-411)
   - ✅ Supports: image/png, image/jpeg, image/svg+xml, text/html
   - ✅ DataFrames and matplotlib plots render correctly

2. ~~**Kernel status indicator** - Show idle/busy/dead state~~ ✅ **COMPLETED**
   - ✅ Visual indicator in toolbar (WorkbookViewer.jsx:1278-1280)
   - ✅ Shows: starting, idle, busy, error, restarting states
   - ✅ Updates in real-time during execution

### P1 - Important for Feature Parity
3. ~~**Streaming stdout/stderr** - Real-time output during execution~~ ✅ **COMPLETED**
   - ✅ Backend uses Server-Sent Events (engine_server.py)
   - ✅ Frontend appends outputs as they arrive (WorkbookViewer.jsx)
   - Perfect for long-running cells with progress indicators

4. ~~**Interrupt execution** - Stop running cell~~ ✅ **COMPLETED**
   - ✅ Backend support (engine_http.rs:329-346)
   - ✅ UI: Interrupt button in toolbar (WorkbookViewer.jsx:1290-1291)
   - ✅ Disables when not busy

5. ~~**DD (double-tap) delete** - Match Jupyter's cell deletion pattern~~ ✅ **COMPLETED**
   - ✅ Proper DD double-tap detection (WorkbookViewer.jsx:526-540)
   - ✅ 500ms window for double-tap

### P2 - Nice to Have
6. **Drag & drop cell reordering** - Visual reordering
7. **Hover-between-cells insert UI** - Plus button between cells
8. **Undo/redo for structural changes** - Cell add/delete/move/type change
9. **Restart kernel & run all** - Combine existing functions
10. **Output label** (`Out [1]:`) - Show execution count on outputs
11. ~~**Notebook file operations** - Rename, delete, duplicate notebooks~~ ✅ **COMPLETED**
    - ✅ Context menu on file tree (right-click)
    - ✅ Backend commands: `rename_file`, `delete_file`, `duplicate_notebook`
    - ✅ Closes tabs for deleted files
    - ✅ Confirmation dialogs for destructive operations

---

### Toolbar Actions

#### Run All
- [x] Run all button runs cells top → bottom in document order (`runAllCells` @ NotebookViewer.jsx:681)
- [ ] "Run all above" / "Run all below" (context menu)
- [ ] Shows per-cell running state + overall progress indicator (only has `isRunningAll` flag)
- [ ] Can interrupt/cancel a long-running run-all
- [ ] Configurable behavior when a cell errors (stop vs continue) - currently continues

#### Clear All
- [x] Clear outputs only (default behavior) (`clearAllOutputs` @ NotebookViewer.jsx:726)
- [x] Option: Clear outputs + reset execution counts (both cleared together)
- [x] Clears stderr/errors (all output types cleared)
- [x] Clears rich outputs (plots, HTML, tables), not just text
- [x] Does NOT delete cell source code

---

### Keyboard Shortcuts

#### Cell Execution
- [x] `Shift+Enter` runs current cell and moves focus to next cell
- [x] If on last cell, `Shift+Enter` inserts a new code cell below
- [x] `Ctrl/Cmd+Enter` runs cell without moving focus
- [x] `Alt+Enter` runs cell and inserts new cell below
- [x] All shortcuts work consistently for both code and markdown cells

#### Cell Management
- [x] `A` - Add cell above (command mode) (NotebookViewer.jsx:517-520)
- [x] `B` - Add cell below (command mode) (NotebookViewer.jsx:521-524)
- [x] `DD` - Delete cell with double-tap (standard Jupyter behavior) (NotebookViewer.jsx:526-540)
- [ ] `X` - Cut cell
- [ ] `C` - Copy cell
- [ ] `V` - Paste cell below
- [x] `M` - Convert cell to markdown (NotebookViewer.jsx:541-544)
- [x] `Y` - Convert cell to code (NotebookViewer.jsx:545-550)

#### Modes
- [x] Command mode vs Edit mode (or equivalent)
- [x] `Escape` to enter command mode
- [x] `Enter` to enter edit mode

---

### File State Management

#### Dirty State Detection
- [x] Triggered by: cell source edits
- [x] Triggered by: cell add/remove
- [x] Triggered by: cell reorder
- [ ] Triggered by: metadata changes
- [x] Visual indicator showing unsaved changes (e.g., dot in tab title)

#### Save/Close Behavior
- [x] On tab/window close with unsaved changes, prompt: Save / Discard / Cancel (NotebookViewer.jsx:774-793)
- [ ] If save fails, user is not allowed to lose work silently (error shown but could lose work)
- [x] Autosave policy defined (interval + on-run + on-blur):
  - **Interval**: Every 3 seconds when dirty (NotebookViewer.jsx:291-304)
  - **On-blur**: When editor loses focus (NotebookViewer.jsx:48-57, 73-81, 196-198)
  - **On-run**: Saves before execution (NotebookViewer.jsx:44, calls onUpdate)
  - Configurable via `autosaveEnabled` prop

#### Recovery
- [ ] Crash recovery / restore last session
- [ ] Conflict handling (file changed on disk by another process)

---

### Cell Insertion UI

- [ ] Hovering between cells reveals "add cell" menu
- [x] Option to add Code cell
- [x] Option to add Markdown cell
- [x] Insert UI is keyboard accessible
- [ ] Insert UI doesn't cause layout jump
- [ ] "Add cell above/below" available via context menu

---

### Cell Reordering (Drag & Drop)

- [ ] Drag handle on each cell (not whole cell draggable)
- [ ] Visual feedback during drag (ghost element, drop indicator)
- [ ] Drag works with multi-select (move multiple cells)
- [x] Reorder updates document order and run-all order
- [x] Reorder marks file as dirty/changed
- [ ] Undo/redo works for reorder operations

---

### Kernel Lifecycle & State

- [x] Kernel start on notebook open (NotebookViewer.jsx:563-588, auto-starts via useEffect)
- [x] Kernel stop/shutdown (per notebook) (NotebookViewer.jsx:590-600, cleanup on unmount)
- [x] Restart kernel (NotebookViewer.jsx:602-641, clears all outputs after restart)
- [ ] Restart kernel & run all (could combine `restartEngine` + `runAllCells`)
- [x] Interrupt execution (stop running cell) (WorkbookViewer.jsx:643-653, engine_http.rs:329-346)
- [x] Kernel status indicator (idle / busy / dead / disconnected) (WorkbookViewer.jsx:1278-1280, 464)
- [x] Per-notebook engine association (or kernel picker if supporting multiple) (engine_server.py manages per-notebook engines)

---

### Cell Types & Editing

#### Types
- [x] Code cells
- [x] Markdown cells
- [ ] Raw cells (optional)

#### Markdown Behavior
- [x] Markdown renders when not in edit mode
- [x] Double-click rendered markdown to edit
- [x] `Shift+Enter` on markdown cell renders it and moves to next

#### Code Editing
- [x] Syntax highlighting (Python at minimum; kernel-driven if possible)
- [x] Language mode detection (works in markdown code blocks)

---

### Output Model

#### Execution Display
- [x] Execution count per code cell (`In [1]:`, `In [2]:`, etc.) (NotebookViewer.jsx:169-171)
- [ ] Output label (`Out [1]:`) - not shown, but could be added

#### Streaming Output
- [x] stdout streams in real-time while cell is running ✅ **IMPLEMENTED**
  - Backend: kernel_server.py:327-446 (Server-Sent Events via `/kernel/execute_stream`)
  - Rust: kernel_http.rs:196-266 (`execute_stream` with SSE parsing)
  - Tauri: lib.rs:309-336 (`execute_cell_stream` command with event emission)
  - Frontend: NotebookViewer.jsx:710-755 (event listener appends outputs as they arrive)
- [x] stderr streams separately (distinct styling) (CellOutput component @ NotebookViewer.jsx:238-245)
- [x] "Clear output" button per cell (NotebookViewer.jsx:161-163, toolbar button when output exists)

#### Rich Outputs
- [x] Images (PNG, JPEG, etc.) (WorkbookViewer.jsx:302-330)
- [x] HTML rendering (text/html mime type) (WorkbookViewer.jsx:347-361)
- [x] SVG rendering (image/svg+xml mime type) (WorkbookViewer.jsx:332-345)
- [x] Tables (DataFrame display - text/html mime type) (same as HTML rendering)
- [x] Plots (matplotlib, plotly, etc. - image/png, application/json mime types)
- [x] Multiple outputs per cell supported (WorkbookViewer.jsx)
- **NOTE: CellOutput component handles all major mime types with priority fallback**

#### Error Handling
- [x] Formatted tracebacks (NotebookViewer.jsx:257-263)
- [x] ANSI color codes stripped from output (stripAnsi function @ NotebookViewer.jsx:232-236)
- [ ] Clickable file/line references in tracebacks (would need traceback parsing)

---

### Notebook File Format & Compatibility

#### .ipynb Support
- [x] Read .ipynb files faithfully
- [x] Write .ipynb files faithfully
- [x] Preserve cell metadata (tags, collapsed state, etc.)
- [x] Preserve outputs in file (optional but typical)
- [x] Preserve notebook-level metadata (kernelspec, language_info)

#### Security / Trust
- [ ] Trust model for HTML/JS outputs
- [ ] Untrusted notebook warnings
- [ ] Output sanitization for untrusted content

---

### Navigation & Search

#### Search
- [ ] Find in current cell
- [ ] Find across entire notebook
- [ ] Find and replace

#### Navigation
- [ ] Jump to cell by number/index
- [ ] Markdown header outline sidebar
- [x] Arrow keys navigate between cells (command mode)

---

### Undo/Redo

- [ ] Undo/redo text edits within cells (Monaco has built-in, but needs testing)
- [ ] Undo/redo structural edits (add/remove/move cells)
- [ ] Undo/redo for clear outputs (nice to have)
- [ ] Undo/redo for cell type conversion

---

### Multi-Cell Selection & Operations

#### Selection
- [x] Click to select single cell
- [ ] Shift+click to select range
- [ ] Cmd/Ctrl+click to toggle selection

#### Bulk Operations
- [ ] Run selected cells
- [ ] Delete selected cells
- [ ] Change type of selected cells (code ↔ markdown)
- [ ] Clear outputs of selected cells
- [ ] Move selected cells up/down
- [ ] Copy/cut/paste selected cells

---

### Cell Toolbar & Context Menu

- [x] Run cell button
- [ ] Stop/interrupt cell button
- [x] Clear output button
- [ ] Duplicate cell
- [ ] Convert cell type
- [x] Delete cell
- [x] Move up/down buttons

---

### Output Display Polish

- [ ] Collapsed/expandable outputs toggle
- [ ] Scroll long outputs (max-height with "expand" option)
- [ ] Execution timing/duration per cell (optional)

---

### UI Polish

- [ ] Sticky toolbar (remains visible on scroll)
- [x] Cell focus/selection highlighting
- [ ] Running cell indicator (spinner, border pulse, etc.)
- [ ] Smooth animations for cell insert/delete/reorder

---

## MVP Parity Bar (Minimum for "works like Jupyter")

**Summary: 85% Complete** - Core editing, execution, rich outputs, streaming, interrupt, and kernel status all work excellently. Missing only: drag reordering, hover-between-cells insert UI, and undo/redo for structural changes.

- [x] **Kernel start/restart/stop/interrupt** - WORKS FULLY ✅
  - Start on open ✓, Stop ✓, Restart ✓ + auto-clears outputs
  - Interrupt execution ✓ (button + backend support)
  - Visible status indicator ✓ (starting/idle/busy/error/restarting)

- [x] **Code + Markdown cells with keyboard shortcuts** - WORKS FULLY ✅
  - All execution shortcuts work (`Shift+Enter`, `Ctrl/Cmd+Enter`, `Alt+Enter`)
  - Command mode (A/B/M/Y/arrows/Escape/Enter) ✓
  - DD double-tap delete ✓ (like Jupyter)

- [x] **`.ipynb` read/write with metadata preservation** - WORKS FULLY ✅
  - Faithful round-trip, preserves cell metadata, notebook metadata, outputs

- [x] **Rich outputs + formatted tracebacks** - WORKS FULLY ✅
  - Formatted tracebacks ✓, ANSI stripping ✓
  - Streaming output ✓ (real-time via Server-Sent Events)
  - Images (PNG, JPEG) ✓, SVG ✓, HTML ✓, tables ✓, plots ✓

- [ ] **Cell insert UI + drag reorder** - PARTIAL (60%)
  - Keyboard insert (A/B) ✓, toolbar buttons ✓
  - **MISSING**: Hover-between-cells insert UI
  - **MISSING**: Drag & drop reordering (has move up/down buttons)

- [ ] **Undo/redo for structural changes** - NOT IMPLEMENTED
  - Monaco has text-level undo, but no structural undo

- [x] **Dirty detection + save/close prompt + autosave** - WORKS FULLY ✅
  - Dirty detection ✓, visual indicator ✓, save prompt ✓
  - Autosave: interval (3s) + on-blur + on-run ✓
  - **MISSING**: Crash recovery (nice-to-have)


## Additional Items to Consider

### Variable/State Inspection
- [ ] Variable explorer panel (list active variables, types, values)
- [ ] Inspect variable details (expand arrays, DataFrames, etc.)
- [ ] Clear/reset namespace without kernel restart

### Code Assistance
- [ ] Autocomplete / IntelliSense (kernel-driven or LSP)
- [ ] Tab completion for variables, methods, file paths
- [ ] Function signature hints / tooltips
- [ ] Docstring popup on hover or `Shift+Tab`
- [ ] Linting / error squiggles (optional)

### Cell Metadata & Tags
- [ ] Edit cell metadata (JSON view)
- [ ] Cell tags (e.g., `skip-execution`, `hide-input`, `hide-output`)
- [ ] Collapsible cell input (hide code, show only output)
- [ ] Collapsible cell output
- [ ] Cell-level comments/notes

### Line Numbers
- [x] Toggle line numbers on/off (per cell or global) - currently always on
- [ ] Go to line within cell

### Magic Commands & Special Syntax
- [ ] Support `%magic` and `%%cell_magic` commands
- [ ] `!shell` command execution
- [ ] `?` and `??` for help/source inspection

### Execution Control
- [ ] Cell execution queue visualization
- [ ] Cancel queued cells (not just interrupt running)
- [ ] Execution dependencies / cell linking (advanced, optional)

### Export & Sharing
- [ ] Export to Python script (.py)
- [ ] Export to HTML
- [ ] Export to PDF
- [ ] Export to Markdown
- [ ] Download notebook
- [ ] Clear outputs before export (option)

### Images & Media in Markdown
- [ ] Drag-and-drop images into markdown cells
- [ ] Paste images from clipboard
- [ ] Embedded vs linked images
- [ ] Render LaTeX/MathJax in markdown cells

### Collaboration (if applicable)
- [ ] Real-time collaborative editing
- [ ] Cursor presence (see other users)
- [ ] Comments on cells
- [ ] Version history / diff view

### Accessibility
- [ ] Screen reader support
- [ ] Keyboard-only navigation (all features accessible without mouse)
- [ ] High contrast mode
- [ ] Customizable font size

### Theming & Customization
- [ ] Light/dark mode toggle
- [ ] Custom themes
- [ ] Configurable keybindings
- [ ] Editor font family/size settings

### Performance & Large Notebooks
- [ ] Virtualized rendering for notebooks with 100+ cells
- [ ] Lazy loading of outputs
- [ ] Large output truncation with "show more"
- [ ] Memory management (clear old outputs option)

### Debugging (advanced)
- [ ] Breakpoint support
- [ ] Step through code
- [ ] Debug console
- [ ] Variable inspection at breakpoint

### Notebook Management

- [x] File browser / notebook list (FileExplorer.jsx:117-354)
  - Tree view with expandable folders
  - File icons by type (📓 .ipynb, 🐍 .py, 📝 .md, etc.)
  - Click to open notebooks in tabs
  - Lazy loading of folder contents
- [x] Create new notebook (FileExplorer.jsx:158-184)
  - "+" button in file explorer header
  - Creates in `/notebooks` directory with inline form
  - Auto-refreshes file list after creation
- [x] Rename notebook (Right-click context menu)
  - Backend: fs.rs:154-168 (`rename_file`)
  - Frontend: FileExplorer.jsx:201-219 (`handleRenameConfirm`)
  - InputDialog component with validation
  - Refreshes file list after rename
- [x] Delete notebook (Right-click context menu)
  - Backend: fs.rs:171-185 (`delete_file`)
  - Frontend: FileExplorer.jsx:221-250 (`handleDeleteFile`)
  - Confirmation dialog before deletion
  - Closes open tabs for deleted files
  - Auto-refreshes file list
- [x] Duplicate notebook (Right-click context menu, notebooks only)
  - Backend: fs.rs:188-230 (`duplicate_notebook`)
  - Frontend: FileExplorer.jsx:252-273 (`handleDuplicateConfirm`)
  - Clears outputs from duplicated notebook
  - Suggests "copy" suffix for new name
- [ ] Move/organize notebooks into folders (drag & drop not implemented)

### Session & Kernel Management
- [ ] View all running kernels
- [ ] Shutdown idle kernels
- [ ] Kernel resource usage (memory, CPU)
- [ ] Multiple kernel support (switch Python version, R, Julia, etc.)

### Miscellaneous
- [ ] Print notebook (print-friendly CSS)
- [ ] Notebook table of contents (auto-generated from headers)
- [ ] Scroll sync between outline and notebook
- [ ] "Scroll to running cell" during execution
- [ ] Recently opened notebooks
- [ ] Notebook templates

---

## Implementation Notes for Remaining Priority Items

### Drag & Drop Cell Reordering (P2)

**Approach**: Add drag handles to cells and use HTML5 drag and drop API or a library like react-beautiful-dnd.

**Implementation**:
```jsx
// Add drag handle to cell toolbar
<div className="cell-toolbar">
  <div className="drag-handle" draggable onDragStart={(e) => handleDragStart(e, index)}>
    ⋮⋮
  </div>
  {/* ... other toolbar buttons */}
</div>

// Handle drag events
const handleDragStart = (e, index) => {
  e.dataTransfer.effectAllowed = 'move';
  e.dataTransfer.setData('text/plain', index);
  setDraggingIndex(index);
};

const handleDrop = (e, targetIndex) => {
  e.preventDefault();
  const sourceIndex = parseInt(e.dataTransfer.getData('text/plain'));
  if (sourceIndex !== targetIndex) {
    moveCell(sourceIndex, targetIndex);
  }
  setDraggingIndex(null);
};
```

### Hover-Between-Cells Insert UI (P2)

**Approach**: Add invisible hover zones between cells that show an "Add Cell" button.

**Implementation**:
```jsx
// Add between each cell
{cells.map((cell, index) => (
  <>
    <div
      className="cell-divider"
      onMouseEnter={() => setHoveredDivider(index)}
      onMouseLeave={() => setHoveredDivider(null)}
    >
      {hoveredDivider === index && (
        <div className="insert-cell-menu">
          <button onClick={() => addCellAt(index, 'code')}>+ Code</button>
          <button onClick={() => addCellAt(index, 'markdown')}>+ Markdown</button>
        </div>
      )}
    </div>
    <Cell key={cell.id} {...cell} />
  </>
))}
```

### Undo/Redo for Structural Changes (P2)

**Approach**: Implement a command pattern with history stack for structural operations.

**Implementation**:
```jsx
const [history, setHistory] = useState([]);
const [historyIndex, setHistoryIndex] = useState(-1);

const executeCommand = (command) => {
  command.execute();
  const newHistory = history.slice(0, historyIndex + 1);
  newHistory.push(command);
  setHistory(newHistory);
  setHistoryIndex(newHistory.length - 1);
};

const undo = () => {
  if (historyIndex >= 0) {
    history[historyIndex].undo();
    setHistoryIndex(historyIndex - 1);
  }
};

const redo = () => {
  if (historyIndex < history.length - 1) {
    history[historyIndex + 1].execute();
    setHistoryIndex(historyIndex + 1);
  }
};
```

---

## Notebook Management Implementation Status

All notebook management features are fully implemented ✅

### Summary

- **Context Menu**: Right-click on any file/workbook in FileExplorer shows rename/delete/duplicate options
- **Rename**: Backend (fs.rs:273-288), Frontend (FileExplorer.jsx with InputDialog)
- **Delete**: Backend (fs.rs:290-305), Frontend (FileExplorer.jsx with confirmation dialog)
- **Duplicate**: Backend (fs.rs:307-350), Frontend (FileExplorer.jsx with name prompt)
- **Tab Management**: Properly handles renaming/deleting open files by updating or closing tabs

---

## Testing Checklist

When implementing features, test with:

- **Rich outputs**:
  - `import matplotlib.pyplot as plt; plt.plot([1,2,3]); plt.show()`
  - `import pandas as pd; pd.DataFrame({'a': [1,2,3]})`
  - `from IPython.display import HTML; HTML('<h1>Test</h1>')`

- **Streaming**:
  - `import time; for i in range(10): print(i); time.sleep(0.5)`

- **Interruption**:
  - `import time; time.sleep(100)` then interrupt

- **Error handling**:
  - `raise ValueError("test error")`
  - `1/0`

- **Keyboard shortcuts**: Test all shortcuts in both command and edit mode

- **Autosave**: Make edits, wait 3s, check file is saved

- **File operations**: Test with notebooks that have:
  - Many cells (100+)
  - Large outputs
  - Unicode/emoji in source
  - Empty cells
  - Cells with metadata

- **Notebook management**:
  - Create notebook with special characters in name
  - Rename open notebook (should update tab or close)
  - Delete notebook that has running kernel (should stop kernel)
  - Delete notebook with unsaved changes (should prompt)
  - Duplicate notebook with large outputs (should clear outputs)
  - Rename to existing name (should show error)
  - Context menu actions work on all file types
