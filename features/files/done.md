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

## Integration

- [x] Files section integrated into sidebar
- [x] Opens files in tabs via navigation system
- [x] Context menu integrated
- [x] Autosave support (shared with workbooks)
