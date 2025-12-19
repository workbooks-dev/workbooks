# Files - Completed

## Files Sidebar Section

- [x] Tree view of all project files
- [x] Shows notebooks folder in FILES section
- [x] Workbooks can be opened from any location (not just Workbooks section)
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
- [x] Enhanced context menu with full options
  - [x] New File (if folder)
  - [x] New Folder (if folder)
  - [x] Rename
  - [x] Delete (with confirmation)
  - [x] Reveal in Finder/Explorer
  - [x] Copy Path (absolute path)
  - [x] Copy Relative Path
  - [x] Get Info (file metadata dialog)
- [x] Support for separators between menu groups
- [x] Support for disabled menu items
- [x] Support for keyboard shortcuts display
- [x] Support for icons
- [x] Context menu positioning near cursor
- [x] Click outside to close
- [x] Escape key to cancel
- [x] VS Code-style menu appearance

## Input Dialog

- [x] Modal dialog for rename/duplicate
- [x] Auto-focus on input
- [x] Text selection on open
- [x] Enter to confirm
- [x] Escape to cancel
- [x] Validation (empty names, duplicates)

## File Info Dialog

- [x] Modal dialog showing file metadata
- [x] Displays file name, path, type, size
- [x] Shows modification and creation dates
- [x] Shows file permissions (read-only vs read & write)
- [x] Human-readable file size formatting
- [x] Formatted date/time display
- [x] Monospace path display with word wrapping

## File Operations Backend

- [x] `list_files()` - Recursive directory listing
- [x] `read_file()` - Read file contents
- [x] `save_file()` - Write file to disk
- [x] `rename_file()` - Rename or move file
- [x] `delete_file()` - Delete file
- [x] `create_new_file()` - Create new empty file
- [x] `create_new_folder()` - Create new folder
- [x] `get_file_info()` - Get detailed file metadata
  - [x] Returns name, path, size, type, dates, permissions
- [x] `reveal_in_finder()` - Reveal file in system file manager
  - [x] macOS support (Finder)
  - [x] Windows support (Explorer)
  - [x] Linux support (file manager)
- [x] File metadata (name, path, size, modified time, created time, permissions)
- [x] Clipboard integration for copying paths

## Environment Variable

- [x] `TETHER_PROJECT_FOLDER` injection
  - [x] Set in all Jupyter kernels
  - [x] Absolute path to project root
  - [x] Available via `os.environ["TETHER_PROJECT_FOLDER"]`
  - [x] Enables portable file paths in workbooks

## File Drop Handling

- [x] Drag and drop file upload
- [x] Drag and drop folder upload (recursive copy)
  - [x] Automatically detects if dropped item is directory
  - [x] Recursively copies entire folder structure
  - [x] Preserves all subdirectories and files
  - [x] Backend: `copy_folder_recursively()` and `save_dropped_folder()`
  - [x] Frontend: Uses `stat()` to check if directory
- [x] `.ipynb` files → Saved to `/notebooks` folder
- [x] Other files → Saved to project root
- [x] Folders → Saved to project root
- [x] Automatic file/folder type detection
- [x] Files appear in correct sidebar section
- [x] Visual drop zone indicator
  - [x] Blue dashed border overlay when dragging
  - [x] Clear messaging about file destination
  - [x] Pointer-events disabled on overlay

## File Search & Filter

- [x] Search files by name
- [x] Real-time filtering
- [x] Recursive search through all subfolders
- [x] Debounced search for better performance
- [x] Shows "no matches" message when search returns nothing
- [x] Search input in Files section header
- [x] Search results show file count
- [x] Path display in search results (shows folder location)

## Subfolder Support

- [x] Tree view with expand/collapse for folders
- [x] Lazy loading of folder contents on expand
- [x] Recursive file tree rendering
- [x] Indentation based on nesting level
- [x] Folder icons (▶/▼) to indicate state
- [x] Search works recursively through all subfolders
- [x] File path display in search results

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

## UX Improvements

- [x] Breadcrumb navigation in FileViewer
  - [x] Shows full file path with folder hierarchy
  - [x] VS Code-style breadcrumb display
  - [x] Path separators between folders
  - [x] Last item (filename) highlighted with bold text
  - [x] Ellipsis overflow handling for long paths
  - [x] Works in both regular FileViewer and Image viewer headers
  - [x] Graceful fallback to simple filename when projectRoot not available

- [x] Focus retention for file/folder creation inputs
  - [x] "+ New File" input retains focus when clicked
  - [x] "+ New Folder" input retains focus when clicked
  - [x] Uses refs and useEffect for reliable focus management
  - [x] Prevents focus loss on button press
