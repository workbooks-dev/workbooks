# File Management

## Overview

Workbooks provides integrated file management for all project files (data, scripts, configs, etc.). Workbooks (`.ipynb` files) are shown separately in the Workbooks section.

## Design Philosophy

**One Place for Everything:**
- Keep data files close to workbooks
- Easy access from sidebar
- No need to switch to Finder/Explorer
- Workbooks can reference files easily

**Environment Variable Integration:**
- `WORKBOOKS_PROJECT_FOLDER` available in all workbooks
- Absolute path to project root
- Reliable file access across machines

## Sidebar Section

**Files Tree View:**
- Shows all project files except `.ipynb`
- Reflects actual folder structure
- Click file → Opens in tab
- Right-click → Context menu

**File Operations:**
- Rename
- Delete
- Duplicate
- Create new file (future)

**Supported File Types:**
- Data: CSV, Excel, JSON, SQLite, Parquet, etc.
- Code: Python (.py), JavaScript, etc.
- Config: YAML, TOML, JSON, .env
- Docs: Markdown, text files
- Any other project files

## FileViewer Component

**Monaco Editor Integration:**
- Syntax highlighting for code files
- Multi-language support
- Line numbers
- Code folding

**Special Viewers:**
- **Markdown** - Preview mode with rendered output
- **Images** - Display image files
- **JSON** - Formatted and syntax highlighted
- **Python** - Full syntax highlighting

**Save Functionality:**
- Cmd/Ctrl+S to save
- Unsaved changes indicator
- Auto-save toggle (shared with workbooks)

## File Drop Behavior

**Drag and Drop:**
- Drop `.ipynb` files → Saved to `/notebooks` folder
- Drop other files → Saved to project root
- Automatic file type detection
- Progress indicator for large files

**Use Cases:**
- Drop CSV data file → Appears in Files section
- Drop notebook → Appears in Workbooks section
- Organize manually into subdirectories

## Environment Variable: WORKBOOKS_PROJECT_FOLDER

**Purpose:**
- Provide absolute path to project root
- Consistent file access across machines
- No hardcoded paths in workbooks

**Availability:**
- Injected into all Jupyter kernels
- Available as `os.environ["WORKBOOKS_PROJECT_FOLDER"]`
- Set before any cell execution

**Usage Example:**
```python
import os
import pandas as pd

# Get project root
project_root = os.environ["WORKBOOKS_PROJECT_FOLDER"]

# Load data file
data_path = os.path.join(project_root, "data.csv")
df = pd.read_csv(data_path)

# Organized projects
sales_path = os.path.join(project_root, "data", "sales", "2024.csv")
df_sales = pd.read_csv(sales_path)
```

**Benefits:**
- Portable workbooks
- No "file not found" errors
- Works on any machine
- Easy to reorganize files

## Context Menu

**Right-click File:**
- Rename - Opens input dialog
- Delete - Confirmation dialog
- Duplicate - Creates copy with new name

**Implementation:**
- `ContextMenu.jsx` component
- Positioned near cursor
- Click outside to close
- Escape key to cancel

**Input Dialog:**
- `InputDialog.jsx` component
- Auto-focus and text selection
- Enter to confirm
- Escape to cancel

## File Operations (Backend)

**Tauri Commands:**
- `list_files(path)` - Recursive directory listing
- `read_file(path)` - Read file contents
- `save_file(path, content)` - Write file
- `rename_file(old_path, new_path)` - Rename/move
- `delete_file(path)` - Delete file
- `create_file(path, content)` - Create new file (future)

**File Metadata:**
```rust
struct FileInfo {
  name: String,
  path: String,
  is_directory: bool,
  size: u64,
  modified: SystemTime,
}
```

## Integration Points

### With Workbooks

**File Access:**
- Workbooks can read any file in project
- Use `WORKBOOKS_PROJECT_FOLDER` for paths
- No special permissions needed

**Example Workflow:**
1. Drop CSV file into Workbooks
2. Appears in Files section
3. Open workbook
4. Load CSV using `WORKBOOKS_PROJECT_FOLDER`
5. Process data

### With Sidebar

**Tree View:**
- `FileExplorer.jsx` component
- Shows nested structure
- Expand/collapse folders
- File type icons

**Recent Files (Future):**
- Track recently opened files
- Quick access list
- Similar to Workbooks recent-use

### With Drag-and-Drop

**Drop Zones:**
- Entire app is drop target
- Visual feedback on drag
- File type detection
- Automatic organization

## Future Enhancements

**Search:**
- Search file contents
- Filter by file type
- Recent files

**File Previews:**
- CSV preview (table view)
- Image thumbnails
- JSON tree view
- Syntax preview for code

**Bulk Operations:**
- Multi-select files
- Batch delete/move
- Folder operations

**Templates:**
- Common data structures
- Example datasets
- Starter scripts
