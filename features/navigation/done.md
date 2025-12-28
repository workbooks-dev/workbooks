# Navigation - Completed

## Tab System Foundation

- [x] Basic tab management in App.jsx
  - [x] `tabs` and `activeTab` state
  - [x] Add/remove/switch tabs
  - [x] Tab data structure (id, type, title, path)

- [x] TabBar component
  - [x] Display list of tabs
  - [x] Close button for each tab
  - [x] Active tab highlighting
  - [x] Autosave toggle control
  - [x] File type icons

- [x] Core tab types
  - [x] `welcome` - Welcome screen
  - [x] `create` - Create project wizard
  - [x] `workbook` - Workbook viewer
  - [x] `file` - General file viewer

- [x] Integration with sidebar
  - [x] Clicking file in Files section opens tab
  - [x] Clicking workbook in Workbooks section opens tab
  - [x] New workbook creates and opens tab

## Native OS Menu Bar

- [x] macOS native menu bar implementation
  - [x] App menu ("workbooks") with About and Quit
  - [x] File menu with New Workbook (Cmd+N), Open Project (Cmd+O), Open in New Window (Cmd+Shift+O)
  - [x] Edit menu with standard editing commands (Undo, Redo, Cut, Copy, Paste, Select All)
  - [x] View menu with Show Runtime Logs (Cmd+Shift+L), Open Logs Folder
  - [x] Window menu with Minimize, Maximize, Close Window
  - [x] Proper macOS menu structure (app menu must be first submenu)
  - [x] Menu event handling and emission to frontend
  - [x] Keyboard shortcuts for all major actions

## Tab State Preservation

- [x] Fixed tab switching state reset bug (2025-12-27)
  - [x] Changed rendering strategy from conditional to hidden rendering
  - [x] All tabs now render simultaneously but inactive ones use `display: none`
  - [x] Components stay mounted when switching tabs
  - [x] Preserves all state including:
    - [x] Cell outputs in workbooks
    - [x] Selected cell and edit mode
    - [x] Scroll position
    - [x] Jupyter kernel state
  - [x] Uses absolute positioning with `absolute inset-0` for tab overlays
  - [x] Keys based on `tab.path || tab.type` for stable component identity
