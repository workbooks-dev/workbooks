## Jupyter Notebook Feature Checklist

**Current Status: 70% MVP Complete** (Last updated: 2025-12-17)

The notebook viewer has solid core functionality - editing, execution, keyboard shortcuts, and file management work well. The main gaps are rich output rendering (images/plots/HTML), streaming output, and some UX polish.

---

## Priority Action Items

To reach full MVP parity with Jupyter, implement these in order:

### P0 - Critical for Basic Usability
1. **Rich output rendering** - Images, HTML, tables, plots
   - Extend `CellOutput` component to handle mime types beyond text/plain
   - Add support for: image/png, image/jpeg, text/html, image/svg+xml
   - DataFrames and matplotlib plots are essential for data science workflows

2. **Kernel status indicator** - Show idle/busy/dead state
   - Add visual indicator in toolbar or per-cell
   - Helps users understand when execution is in progress

### P1 - Important for Feature Parity
3. ~~**Streaming stdout/stderr** - Real-time output during execution~~ ✅ **COMPLETED**
   - ✅ Backend uses Server-Sent Events (kernel_server.py:327-446)
   - ✅ Frontend appends outputs as they arrive (NotebookViewer.jsx:710-755)
   - Perfect for long-running cells with progress indicators

4. **Interrupt execution** - Stop running cell
   - Backend support in kernel_server.py (kernel.interrupt_kernel())
   - UI: Stop button in toolbar + keyboard shortcut

5. **DD (double-tap) delete** - Match Jupyter's cell deletion pattern
   - Replace Shift+D with proper DD double-tap detection

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
- [x] `A` - Add cell above (command mode) (NotebookViewer.jsx:332)
- [x] `B` - Add cell below (command mode) (NotebookViewer.jsx:337)
- [x] `Shift+D` - Delete cell (NotebookViewer.jsx:342) - **NOTE: Not DD double-tap like Jupyter**
- [ ] `DD` - Delete cell with double-tap (standard Jupyter behavior)
- [ ] `X` - Cut cell
- [ ] `C` - Copy cell
- [ ] `V` - Paste cell below
- [x] `M` - Convert cell to markdown (NotebookViewer.jsx:347)
- [x] `Y` - Convert cell to code (NotebookViewer.jsx:352)

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

- [x] Kernel start on notebook open (NotebookViewer.jsx:368-388, auto-starts via useEffect)
- [x] Kernel stop/shutdown (per notebook) (NotebookViewer.jsx:390-401, cleanup on unmount)
- [x] Restart kernel (NotebookViewer.jsx:403-421, clears all outputs after restart)
- [ ] Restart kernel & run all (could combine `restartKernel` + `runAllCells`)
- [ ] Interrupt execution (stop running cell) - no backend support yet
- [ ] Kernel status indicator (idle / busy / dead / disconnected) - only tracks `kernelStartedRef`
- [x] Per-notebook kernel association (or kernel picker if supporting multiple) (kernel_server.py manages per-notebook kernels)

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
- [ ] Images (PNG, JPEG, etc.) - only handles text/plain currently
- [ ] HTML rendering (text/html mime type)
- [ ] SVG rendering (image/svg+xml mime type)
- [ ] Tables (DataFrame display - text/html mime type)
- [ ] Plots (matplotlib, plotly, etc. - image/png, application/json mime types)
- [x] Multiple outputs per cell supported (NotebookViewer.jsx:215-221)
- **NOTE: CellOutput component only handles text/plain, needs mime type routing**

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

**Summary: 70% Complete** - Core editing and execution work well. Missing: rich output rendering, streaming, kernel status UI, drag reordering, and undo/redo.

- [x] **Kernel start/restart/stop** - WORKS (interrupt ✗, status UI ✗)
  - Start on open ✓, Stop ✓, Restart ✓ + auto-clears outputs
  - Missing: interrupt execution, visible status indicator (idle/busy/dead)

- [x] **Code + Markdown cells with keyboard shortcuts** - WORKS
  - All execution shortcuts work (`Shift+Enter`, `Ctrl/Cmd+Enter`, `Alt+Enter`)
  - Command mode (A/B/M/Y/arrows/Escape/Enter) ✓
  - Delete is Shift+D (not DD like Jupyter)

- [x] **`.ipynb` read/write with metadata preservation** - WORKS
  - Faithful round-trip, preserves cell metadata, notebook metadata, outputs

- [ ] **Rich outputs + formatted tracebacks** - PARTIAL (70%)
  - Formatted tracebacks ✓, ANSI stripping ✓
  - Streaming output ✅ **NOW WORKS** (real-time via Server-Sent Events)
  - **MISSING**: Images, HTML, SVG, tables, plots (only text/plain works)

- [ ] **Cell insert UI + drag reorder** - PARTIAL (50%)
  - Keyboard insert (A/B) ✓, toolbar buttons ✓
  - **MISSING**: Hover-between-cells UI
  - **MISSING**: Drag & drop reordering (has move up/down buttons)

- [ ] **Undo/redo for structural changes** - NOT IMPLEMENTED
  - Monaco has text-level undo, but no structural undo

- [x] **Dirty detection + save/close prompt + autosave** - WORKS (crash recovery ✗)
  - Dirty detection ✓, visual indicator ✓, save prompt ✓
  - Autosave: interval (3s) + on-blur + on-run ✓
  - **MISSING**: Crash recovery


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

## Implementation Notes for Priority Items

### Rich Output Rendering (P0)

The current `CellOutput` component (NotebookViewer.jsx:230-267) only handles:
- `text/plain` - rendered as `<pre>`
- Basic error formatting with ANSI stripping

To add rich outputs, extend the component to check `output.data` mime types:

```jsx
function CellOutput({ output }) {
  if (output.output_type === "execute_result" || output.output_type === "display_data") {
    const data = output.data;

    // Priority order: richest format first
    if (data["image/png"]) {
      return <img src={`data:image/png;base64,${data["image/png"]}`} />;
    }
    if (data["image/jpeg"]) {
      return <img src={`data:image/jpeg;base64,${data["image/jpeg"]}`} />;
    }
    if (data["image/svg+xml"]) {
      return <div dangerouslySetInnerHTML={{ __html: data["image/svg+xml"] }} />;
    }
    if (data["text/html"]) {
      // For DataFrames, matplotlib HTML output, etc.
      return <div dangerouslySetInnerHTML={{ __html: data["text/html"] }} />;
    }
    if (data["text/plain"]) {
      return <pre>{data["text/plain"]}</pre>;
    }
  }
  // ... existing stream/error handling
}
```

**Security note**: Using `dangerouslySetInnerHTML` requires careful consideration. May want to:
- Add a trust model (like Jupyter's trusted notebooks)
- Sanitize HTML with a library like DOMPurify
- Or render in a sandboxed iframe

### Streaming Output (P1)

Current implementation: `execute_cell` waits for completion, then returns all outputs at once.

**Backend changes needed** (kernel_server.py):
1. Change execute endpoint to return immediately with a task ID
2. Add WebSocket endpoint for streaming IOPub messages
3. Client connects to WS and receives messages in real-time

**Frontend changes** (NotebookViewer.jsx):
1. Open WebSocket when executing cell
2. Append to `cell.outputs` as messages arrive
3. Update UI incrementally (React state updates)
4. Handle completion/error messages to close stream

**Alternative**: Server-Sent Events (SSE) if one-way streaming is sufficient.

### Kernel Status Indicator (P0)

Track kernel state in NotebookViewer component:

```jsx
const [kernelStatus, setKernelStatus] = useState('idle'); // idle, busy, dead, starting

// Update status based on:
// - Starting: when startKernel() is called
// - Busy: when executing cell
// - Idle: when execution completes
// - Dead: when kernel crashes or stops

// Add to toolbar:
<div className={`kernel-status ${kernelStatus}`}>
  <span className="status-dot"></span>
  {kernelStatus}
</div>
```

For proper status tracking, kernel_server.py should expose a `/status` endpoint or include status in execution responses.

### Interrupt Execution (P1)

**Backend** (kernel_server.py):
```python
@app.post("/interrupt/{notebook_path}")
async def interrupt_kernel(notebook_path: str):
    kernel_manager = get_kernel_manager(notebook_path)
    if kernel_manager:
        kernel_manager.interrupt_kernel()
        return {"status": "interrupted"}
```

**Frontend** (NotebookViewer.jsx):
```jsx
const interruptExecution = async () => {
  try {
    await invoke("interrupt_kernel", { notebookPath });
    setKernelStatus('idle');
  } catch (err) {
    console.error("Failed to interrupt:", err);
  }
};

// Add to toolbar:
<button onClick={interruptExecution} disabled={kernelStatus !== 'busy'}>
  ⬛ Interrupt
</button>
```

**Rust** (kernel_http.rs):
Add `interrupt_kernel` command that POSTs to kernel_server.

---

## Notebook Management Implementation Guide

### Context Menu for Files

To implement rename/delete/duplicate, add a context menu to file tree items:

**FileExplorer.jsx** - Add right-click handler:
```jsx
function FileTreeItem({ file, level, onFileClick, onFileAction }) {
  const [showContextMenu, setShowContextMenu] = useState(false);
  const [contextMenuPos, setContextMenuPos] = useState({ x: 0, y: 0 });

  const handleContextMenu = (e) => {
    e.preventDefault();
    setContextMenuPos({ x: e.clientX, y: e.clientY });
    setShowContextMenu(true);
  };

  return (
    <>
      <div
        className="tree-item"
        onClick={handleToggle}
        onContextMenu={handleContextMenu}
      >
        {/* ... existing content ... */}
      </div>

      {showContextMenu && (
        <ContextMenu
          x={contextMenuPos.x}
          y={contextMenuPos.y}
          onClose={() => setShowContextMenu(false)}
          items={[
            { label: "Rename", action: () => onFileAction('rename', file) },
            { label: "Duplicate", action: () => onFileAction('duplicate', file) },
            { label: "Delete", action: () => onFileAction('delete', file) },
          ]}
        />
      )}
    </>
  );
}
```

### Rename Notebook

**Backend** (fs.rs):
```rust
/// Rename a file or notebook
pub fn rename_file(old_path: &Path, new_name: &str) -> Result<String> {
    let parent = old_path.parent()
        .context("Failed to get parent directory")?;

    let new_path = parent.join(new_name);

    if new_path.exists() {
        anyhow::bail!("A file with that name already exists");
    }

    fs::rename(old_path, &new_path)
        .context("Failed to rename file")?;

    Ok(new_path.to_string_lossy().to_string())
}
```

**Frontend** (FileExplorer.jsx):
```jsx
const handleRename = async (file) => {
  const newName = await showRenameDialog(file.name);
  if (!newName) return;

  try {
    const newPath = await invoke("rename_file", {
      oldPath: file.path,
      newName: newName,
    });

    // If this notebook is open in a tab, update the tab path
    updateTabPath(file.path, newPath);

    // Refresh file list
    await loadRootFiles();
  } catch (err) {
    console.error("Failed to rename:", err);
    showError(err.toString());
  }
};
```

**Important**: When renaming an open notebook, update the corresponding tab's path in App.jsx state, or close the tab and prompt to reopen.

### Delete Notebook

**Backend** (fs.rs):
```rust
/// Delete a file or notebook
pub fn delete_file(file_path: &Path) -> Result<()> {
    if !file_path.exists() {
        anyhow::bail!("File does not exist");
    }

    if file_path.is_dir() {
        fs::remove_dir_all(file_path)
            .context("Failed to delete directory")?;
    } else {
        fs::remove_file(file_path)
            .context("Failed to delete file")?;
    }

    Ok(())
}
```

**Frontend** (FileExplorer.jsx):
```jsx
const handleDelete = async (file) => {
  const confirmed = await ask(
    `Are you sure you want to delete "${file.name}"? This cannot be undone.`,
    {
      title: "Delete File",
      kind: "warning",
      okLabel: "Delete",
      cancelLabel: "Cancel",
    }
  );

  if (!confirmed) return;

  try {
    await invoke("delete_file", { filePath: file.path });

    // If this notebook is open in a tab, close the tab
    closeTabByPath(file.path);

    // Refresh file list
    await loadRootFiles();
  } catch (err) {
    console.error("Failed to delete:", err);
    showError(err.toString());
  }
};
```

**Important**: If the notebook has a running kernel, stop it before deletion. Check if file is open in tabs and close those tabs.

### Duplicate Notebook

**Backend** (fs.rs):
```rust
/// Duplicate a notebook with a new name
pub fn duplicate_notebook(source_path: &Path, new_name: &str) -> Result<String> {
    let parent = source_path.parent()
        .context("Failed to get parent directory")?;

    // Ensure new name has .ipynb extension
    let new_name = if new_name.ends_with(".ipynb") {
        new_name.to_string()
    } else {
        format!("{}.ipynb", new_name)
    };

    let target_path = parent.join(&new_name);

    if target_path.exists() {
        anyhow::bail!("A notebook with that name already exists");
    }

    // Read source notebook
    let content = fs::read_to_string(source_path)
        .context("Failed to read source notebook")?;

    // Parse and clear outputs (optional - makes duplicates cleaner)
    let mut notebook: serde_json::Value = serde_json::from_str(&content)
        .context("Invalid notebook JSON")?;

    if let Some(cells) = notebook["cells"].as_array_mut() {
        for cell in cells {
            if cell["cell_type"] == "code" {
                cell["outputs"] = serde_json::json!([]);
                cell["execution_count"] = serde_json::json!(null);
            }
        }
    }

    // Write to new file
    let content = serde_json::to_string_pretty(&notebook)
        .context("Failed to serialize notebook")?;

    fs::write(&target_path, content)
        .context("Failed to write duplicate notebook")?;

    Ok(target_path.to_string_lossy().to_string())
}
```

**Frontend** (FileExplorer.jsx):
```jsx
const handleDuplicate = async (file) => {
  // Suggest a name like "notebook copy.ipynb" or "notebook (2).ipynb"
  const baseName = file.name.replace('.ipynb', '');
  const suggestedName = `${baseName} copy`;

  const newName = await showRenameDialog(suggestedName);
  if (!newName) return;

  try {
    const newPath = await invoke("duplicate_notebook", {
      sourcePath: file.path,
      newName: newName,
    });

    console.log("Duplicated to:", newPath);

    // Refresh file list
    await loadRootFiles();

    // Optionally, open the new notebook
    onOpenNotebook(newPath, "notebook");
  } catch (err) {
    console.error("Failed to duplicate:", err);
    showError(err.toString());
  }
};
```

### Tab Management Considerations

When implementing file operations that affect open notebooks:

1. **Rename**: Update tab path, or close and prompt to reopen
2. **Delete**: Force-close the tab (with unsaved changes prompt if dirty)
3. **Move** (if implemented): Update tab path

Add helper functions in App.jsx:
```jsx
function updateTabPath(oldPath, newPath) {
  setTabs(tabs.map(tab =>
    tab.path === oldPath ? { ...tab, path: newPath } : tab
  ));
}

function closeTabByPath(filePath) {
  const tab = tabs.find(t => t.path === filePath);
  if (tab) {
    handleTabClose(tab.id);
  }
}
```

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
