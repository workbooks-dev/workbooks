# Files - To Do

## Priority

- [ ] File list should stay current with the folder (it does not)
- [ ] Files need to able to be dragged into folders and subfolders (drag and drop to move files)
- [ ] Renaming files or folders should happen inline (just like vscode)
- [ ] Notebooks in the Workbooks section should be able to be renamed.
- [ ] when a file is open, the title should be editable in the viewer. 
- [ ] If a filename is changed, the scheduler should be auto updated
- [x] + New Folder needs to retain focus (the input loses focus when the button is pressed)
- [x] Breadcrumb navigation in FileViewer (like VS Code)
- [x] control+click (or right click) should have a menu like vscode and styles matching the other menu in the main viewport with
  - [x] New file (if folder)
  - [x] New folder (if folder)
  - [ ] Refresh (folder list)
  - [ ] Find in folder
  - [ ] toggle file size (toggles all file sizes)
  - [ ] Open with (if file)
  - [x] Get info (like macos info if possible)
  - [x] Reveal in Finder
  - [x] Copy Path
  - [x] Copy Relative Path
  - [x] Rename
  - [x] Delete

## FileViewer Enhancements

- [ ] Better file type support
  - [ ] SQLite database browser
  - [ ] Parquet file preview
  - [ ] Excel file viewer
  - [ ] PDF viewer

- [ ] Image viewer enhancements
  - [ ] Basic metadata display (dimensions, file size)
  - [ ] Pan/drag support for zoomed images

- [ ] CSV enhancements
  - [ ] Basic filtering
  - [ ] Edit support (future)
  - [ ] Show all rows option (with warning)

## File Drop Improvements

- [ ] Upload progress
  - [ ] Progress bar for large files
  - [ ] Cancel upload option
  - [ ] Success/error messages

- [ ] Batch file upload enhancements
  - [ ] Preserve folder structure (optional)
  - [ ] Summary of uploaded files

## File Operations

- [ ] File templates
  - [ ] Common file types with starter content (CSV headers, JSON schema, .py template, .md template)
  - [ ] Custom templates

- [ ] Move files between folders
  - [ ] Drag and drop within tree
  - [ ] Move to folder dialog
  - [ ] Update all references (future)

- [ ] Bulk operations
  - [ ] Multi-select files
  - [ ] Batch delete
  - [ ] Batch move

## Search & Filter

- [ ] Advanced search
  - [ ] Filter by file type
  - [ ] Search file contents (future)
  - [ ] File size filters

- [ ] Recent files tracking
  - [ ] Track last 20 opened files
  - [ ] Quick access list
  - [ ] Show in sidebar (optional)

## File Previews

- [ ] Thumbnail previews for images
- [ ] First few lines for text files
- [ ] Row count for CSV/Excel
- [ ] Size and modified date display

## Integration

- [ ] Link detection in workbook outputs
  - [ ] Detect file paths in output
  - [ ] Make clickable
  - [ ] Open in FileViewer

- [ ] File references tracking
  - [ ] Track which workbooks use which files
  - [ ] Show in file info
  - [ ] Dependency visualization (future)

## Performance

- [ ] Lazy loading for large file trees
- [ ] Virtual scrolling for file lists
- [ ] Efficient file watching
- [ ] Cache file metadata

## UX Improvements

- [ ] File icons for more types
- [ ] Better context menu positioning
- [ ] Keyboard shortcuts for file operations
