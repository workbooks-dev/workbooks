# Files - Completed

## Files Sidebar Section

- [x] Tree view of all project files
- [x] Filters out .ipynb files (shown in Workbooks)
- [x] Reflects actual folder structure
- [x] Expand/collapse folders
- [x] File type icons
- [x] Click to open file in tab

## FileViewer Component

- [x] Monaco editor integration
- [x] Syntax highlighting for multiple languages
  - [x] Python
  - [x] JavaScript
  - [x] JSON
  - [x] Markdown
  - [x] YAML
  - [x] And more
- [x] Line numbers
- [x] Code folding
- [x] Markdown preview mode
  - [x] Rendered markdown display
  - [x] Toggle between edit/preview
- [x] Save functionality (Cmd/Ctrl+S)
- [x] Unsaved changes tracking
- [x] Image viewer for PNG, JPG, SVG, GIF, WebP
  - [x] Display images directly
  - [x] Zoom functionality (25% - 400%)
  - [x] Reset zoom button
- [x] CSV preview/viewer
  - [x] Table view with sortable columns
  - [x] Row/column count display
  - [x] Toggle between table and raw CSV
  - [x] Performance optimized (shows first 1000 rows)
  - [x] Numeric and string sorting
- [x] JSON tree viewer
  - [x] Expandable/collapsible tree structure
  - [x] Syntax highlighting for values
  - [x] Type-based color coding
  - [x] Toggle between tree and raw JSON
  - [x] Auto-expand first 2 levels
  - [x] Shows array/object size previews

## Context Menu

- [x] Right-click file operations
- [x] Rename file
- [x] Delete file (with confirmation)
- [x] Duplicate file
- [x] Context menu positioning near cursor
- [x] Click outside to close
- [x] Escape key to cancel

## Input Dialog

- [x] Modal dialog for rename/duplicate
- [x] Auto-focus on input
- [x] Text selection on open
- [x] Enter to confirm
- [x] Escape to cancel
- [x] Validation (empty names, duplicates)

## File Operations Backend

- [x] `list_files()` - Recursive directory listing
- [x] `read_file()` - Read file contents
- [x] `save_file()` - Write file to disk
- [x] `rename_file()` - Rename or move file
- [x] `delete_file()` - Delete file
- [x] `create_new_file()` - Create new empty file
- [x] `create_new_folder()` - Create new folder
- [x] File metadata (name, path, size, modified time)

## Environment Variable

- [x] `TETHER_PROJECT_FOLDER` injection
  - [x] Set in all Jupyter kernels
  - [x] Absolute path to project root
  - [x] Available via `os.environ["TETHER_PROJECT_FOLDER"]`
  - [x] Enables portable file paths in workbooks

## File Drop Handling

- [x] Drag and drop file upload
- [x] `.ipynb` files → Saved to `/notebooks` folder
- [x] Other files → Saved to project root
- [x] Automatic file type detection
- [x] Files appear in correct sidebar section
- [x] Visual drop zone indicator
  - [x] Blue dashed border overlay when dragging
  - [x] Clear messaging about file destination
  - [x] Pointer-events disabled on overlay

## File Search & Filter

- [x] Search files by name
- [x] Real-time filtering
- [x] Shows "no matches" message when search returns nothing
- [x] Search input in Files section header

## File Creation

- [x] Create new file button
- [x] Create new folder button
- [x] Inline creation forms
- [x] Input validation
- [x] Auto-refresh file list after creation
- [x] Support for any file type

## Integration

- [x] Files section integrated into sidebar
- [x] Opens files in tabs via navigation system
- [x] Context menu integrated
- [x] Autosave support (shared with workbooks)
