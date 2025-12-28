# Workbooks Main Todo List


## Critical issues

**All critical issues have been resolved!**

**FIXED** - The following issues have been resolved:

0. **Development server port conflict** - Fixed port 1420 already in use error
   - Root cause: Previous dev server instances not properly terminated
   - Fix: Kill existing processes using `pkill -f "npm run tauri dev" && pkill -f "vite"`
   - Solution: If dev server won't start, check for existing processes and terminate them
   - Date: Dec 27, 2025

1. **Cell source type error** - Fixed `cell.source.join is not a function` error in WorkbookViewer.jsx:1277
   - Root cause: Jupyter notebooks can have cell.source as either a string or an array
   - Fix: Updated `updateCell()` to use the existing `getCellSourceAsString()` helper function that safely handles both formats
   - Location: src/components/WorkbookViewer.jsx:1277

2. **"No engine found for this workbook" error** - Fixed race condition in engine lookup
   - Root cause: Workbook paths weren't being normalized, so `/path/to/notebook.ipynb` and `/path/to//notebook.ipynb` were treated as different engines
   - Fix: Added `normalize_path()` function that uses `os.path.abspath()` and `os.path.normpath()` to ensure consistent dictionary keys
   - Locations updated in engine_server.py:
     - start_engine (line 183)
     - execute_code (line 362)
     - execute_code_stream (line 544)
     - stop_engine (line 713)
     - interrupt_engine (line 733)
     - restart_engine (line 751)
     - execute_all_cells (line 801)
     - complete_code (line 1009)
   - Added logging to show available engines when lookup fails for easier debugging

## High Priority Features

### AI Assistant Enhancements

- [x] **Notebook change visibility and approval** (COMPLETED - Dec 27, 2025)
  - Full diff modal with cell-by-cell comparison
  - Approve/reject flow for AI notebook modifications
  - Version history and revert functionality
  - See: features/ai-assistant/done.md and features/workbooks/done.md
- [ ] **AI Chat streaming responses** - Investigate why Claude Code responses may not be outputting
  - Verify Claude Code CLI is installed and working
  - Check streaming event handling in AiChatPanel.jsx
  - Review error logs for streaming failures

- [x] **Enable Claude Code to run notebooks** (COMPLETED - Dec 27, 2025)
  - Claude can now execute notebooks using `workbooks run` command via Bash tool
  - Execution output visible in chat for iteration
  - System prompt updated to guide Claude on running notebooks
  - See: features/ai-assistant/done.md

- [ ] **AI-driven notebook operations**
  - [x] Create new notebooks (with naming suggestions based on task)
  - [x] Update existing notebooks (with confirmation)
  - [ ] Delete notebooks (with user confirmation modal)
  - [x] Run notebooks and stream execution results to chat
  - [ ] Schedule notebooks via cron expressions from chat
  - [ ] Prompt for environment variables if needed (modal UI)

- [ ] **Session rename functionality**
  - Allow users to rename chat sessions for better organization
  - Update session title in database and UI
  - Also tracked in: features/ai-assistant/todo.md

- [ ] **Session export and sharing**
  - Copy session link/ID to clipboard for opening elsewhere
  - Export chat session to markdown or JSON
  - Import session from file or link

### Notebook execution

- [ ] +Code needs to be added one cell below current cell (if in current cell), otherwise it should be pushed to the end of the notebook


### UX Improvements
- [x] **Resizable and collapsible panels** (COMPLETED - Dec 27, 2025)
  - All three panels (left sidebar, AI chat, file viewer) are now resizable and collapsible
  - VS Code-style panel toggles in top right corner (icon-only buttons)
  - Standard keyboard shortcuts: Cmd+B (left sidebar), Cmd+J (AI chat), Cmd+Shift+B (right panel)
  - Panel sizes and visibility persisted to localStorage
  - Active panels shown with blue icon color
  - Maximum workspace flexibility - hide any combination of panels
  - See: features/changelog.md

- [ ] **Context-aware chat switching**
  - When opening a notebook that was previously closed, load notebook-specific context
  - When switching from one notebook to another, update chat context
  - Option 1: Auto-switch to notebook-specific chat session
  - Option 2: Update focused file context in current session
  - Needs UX decision on best approach

- [x] **Notebook labels for user-friendly names** (COMPLETED - Dec 27, 2025)
  - Friendly labels/titles displayed instead of filenames
  - Labels shown in sidebar, table view, and tab titles
  - Click-to-edit UI in WorkbookViewer header
  - Stored in notebook metadata, fallback to filename
  - See: features/workbooks/done.md

- [ ] **File tabs positioning review**
  - Verify: File tabs should be above file viewer, not above chat
  - Current state may already be correct with AI-first redesign
  - Needs UI/UX testing

- [ ] **No open file**
  - Allow placeholder text instead of collapsing the area and making chat super wide

### Search & Navigation
- [ ] **Universal search across project**
  - Search all workbooks, files, and directories
  - Quick command palette (Cmd+P style)
  - Search notebook contents (cells, outputs, metadata)
  - Search file contents in Files section
  - Currently only file name search exists

- [ ] **Open project in Finder/Explorer**
  - Menu item: "Reveal in Finder" (macOS) / "Show in Explorer" (Windows)
  - Keyboard shortcut (Cmd+Shift+R or similar)
  - Right-click context menu option in sidebar
  - Opens project root folder in system file browser

## Completed Recently ✅

- [x] **Cell source TypeError fixed** (Dec 27, 2025)
  - Fixed `newCells[index].source.join is not a function` error
  - Added support for both array and string cell source formats
  - See details in "Critical issues" section above

- [x] **"No engine found" error fixed** (Dec 27, 2025)
  - Fixed race condition caused by path normalization issues
  - See details in "Critical issues" section above

- [x] **Auto-start chat on project open** (Dec 27, 2025)
  - Fresh chat session automatically created when opening a project
  - Previous chat sessions preserved in history sidebar
  - No manual session creation needed
  - See: features/changelog.md

- [x] **Auto-open files when Claude creates/edits them** (Dec 27, 2025)
  - Files automatically appear in UI when Claude uses Write/Edit tools
  - See: features/changelog.md

- [x] **Model selection persistence** (Dec 27, 2025)
  - Each chat session remembers which model was used
  - Expanded model list with all Claude variants
  - See: features/changelog.md

---

**Note:** For feature-specific todos, see individual feature directories:
- AI Assistant: features/ai-assistant/todo.md
- Navigation: features/navigation/todo.md
- Workbooks: features/workbooks/todo.md
- Files: features/files/todo.md
- Secrets: features/secrets/todo.md
- Schedule: features/schedule/todo.md
- State: features/state/todo.md