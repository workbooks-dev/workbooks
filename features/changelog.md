# Workbooks Development Changelog

This file tracks major features and improvements as they're completed.

## Recent Completions

### December 2025

**Tab State Preservation - Fixed State Reset Bug (Dec 27, 2025)**
- **Bug Fix**: Tab switching no longer clears workbook outputs and state
  - **Root Cause**: Conditional rendering was unmounting/remounting components
  - **Solution**: Changed to hidden rendering strategy
    - All tabs now render simultaneously in the DOM
    - Inactive tabs hidden with CSS (`display: none`)
    - Components stay mounted when switching tabs
  - **Preserved State**:
    - Cell outputs and execution results
    - Selected cell and edit mode
    - Scroll position
    - Jupyter kernel state
  - **Implementation**:
    - Use `absolute inset-0` positioning for tab overlays
    - Toggle visibility with `block/hidden` CSS classes
    - Key components by `tab.path || tab.type` for stable identity
  - **Files Modified**: `src/App.jsx` (lines 1183-1226)

**Files: All/Compact View Toggle (Dec 28, 2025)**
- **Feature**: Added view mode toggle to Files section for cleaner workspace
  - **Segmented Control**: Professional toggle between "All" and "Compact" modes
    - Matches app style guide with blue highlights and clean design
    - Always visible at top of Files section
  - **All Mode (Default)**: Shows every file in the project
    - Complete visibility of entire file structure
    - No filtering applied
  - **Compact Mode**: Shows only automation-relevant files
    - ✅ Notebooks (.ipynb) - the automation scripts
    - ✅ Data files (CSV, JSON, Parquet, etc.) - inputs and outputs
    - ✅ Downloads and generated files
    - ❌ Python source (.py, .pyc, .pyo, .pyd) - implementation details
    - ❌ Config files (pyproject.toml, package.json, etc.) - project setup
    - ❌ Dev folders (.venv, node_modules, __pycache__, .git, .workbooks, etc.)
  - **Smart Filtering Logic**: Filters at all levels
    - Root directory listing
    - Nested folders when expanded
    - Search results
    - Consistent behavior throughout file tree
  - **Persistent Preference**: Choice saved per project
    - Stored in localStorage as `workbooks_file_view_mode_{projectRoot}`
    - Automatically restored on project reopen
  - **Real-time Updates**: Files refresh immediately when toggling
- **Use Case**: Focus on automation workflows without clutter
  - Compact mode ideal for working with data pipelines
  - All mode available when full project access needed
  - Reduces visual noise while maintaining access to everything
- **Files Modified**: src/components/Sidebar.jsx
- **Documentation**: features/files/done.md

**AI Notebook Change Approval System (Dec 27, 2025)**
- **Diff Modal with Visual Changes**: When Claude modifies a notebook, a beautiful diff modal shows all changes
  - Cell-by-cell comparison with color coding (emerald=added, blue=modified, red=deleted)
  - Side-by-side before/after view for modified cells
  - Summary counts showing additions, modifications, deletions
  - Works like Cursor/Windsurf/Antigravity change approval UX
  - Location: `src/components/NotebookDiffModal.jsx`
- **Approve/Reject Workflow**: Users have full control over AI changes
  - "Approve & Apply" button saves changes and opens the notebook
  - "Reject Changes" button reverts to previous version
  - Closing the modal (X or click outside) defaults to rejection for safety
  - No changes applied without explicit user approval
- **Version History System**: Automatic snapshots enable safe experimentation
  - Versions saved to `.workbooks/versions/{notebook_name}/{timestamp}.ipynb`
  - Six Tauri commands for managing versions (save, list, get, revert, cleanup)
  - Backend functions in `src-tauri/src/fs.rs:622-776`
  - All commands already registered in `src-tauri/src/lib.rs`
- **AI Chat Integration**: Seamless interception of notebook modifications
  - Detects Write/Edit tool use on `.ipynb` files
  - Saves current version before modification
  - Loads both old and new content for comparison
  - Triggers modal automatically after Claude finishes
  - Location: `src/components/AiChatPanel.jsx:426-440`
- **App-Level Coordination**: Central approval flow management
  - Modal state and handlers in `src/App.jsx`
  - Proper cleanup and error handling
  - Integration with file opening system
- **Problem Solved**: Users were uncomfortable with Claude making notebook changes blindly
  - Previously, notebooks updated immediately with no visibility or control
  - Now, every change is reviewed before being applied
  - Version history provides safety net for experimentation
  - Familiar UX from other AI coding tools builds confidence
- **Files Modified**:
  - `src/components/NotebookDiffModal.jsx` - New diff modal component
  - `src/App.jsx` - Added modal state and approval handlers
  - `src/components/AiChatPanel.jsx` - Already had integration hooks
  - `src-tauri/src/fs.rs` - Version history functions (already existed)
  - `features/ai-assistant/done.md` - Updated documentation
  - `features/ai-assistant/todo.md` - Marked as complete

**Force Restart and Stuck Kernel Recovery (Dec 27, 2025)**
- **Force Restart Button**: New UI button to kill and restart stuck kernels without closing the app
  - Always available (not disabled like normal Restart)
  - Highlighted in red when engine is in error state or not ready
  - Kills kernel process with SIGKILL, then starts fresh
  - Clears all outputs and execution state after restart
  - Location: `src/components/WorkbookViewer.jsx`
- **Backend Force Restart Endpoint**: New `/engine/force_restart` endpoint in engine server
  - Gets kernel PID and forcefully kills with SIGKILL
  - Falls back to graceful shutdown if process already dead
  - Waits 500ms before starting new kernel to let OS clean up
  - Location: `src-tauri/engine_server.py:801-843`
- **Orphaned Kernel Cleanup**: New `/cleanup/orphaned_kernels` endpoint
  - Finds all ipykernel_launcher processes on the system
  - Identifies which are managed by the engine server
  - Kills all orphaned/unmanaged kernels with SIGKILL
  - Returns count of killed processes and any errors
  - Location: `src-tauri/engine_server.py:181-262`
- **Problem Solved**: Users no longer need to close and restart the entire app when a kernel gets stuck
  - Previously, stuck kernels (consuming high CPU, not responding to interrupt) required app restart
  - Now, Force Restart button provides instant recovery
  - Cleanup endpoint can be called to kill all orphaned kernels from previous sessions
- **Files Modified**:
  - `src-tauri/engine_server.py` - Added force_restart and cleanup endpoints
  - `src-tauri/src/engine_http.rs` - Added HTTP client functions
  - `src-tauri/src/lib.rs` - Added Tauri commands and registered them
  - `src/components/WorkbookViewer.jsx` - Added forceRestartEngine function and UI button

**Workbook Labels for User-Friendly Names (Dec 27, 2025)**
- **Custom labels instead of filenames**: Workbooks now display meaningful names throughout the UI
  - Labels stored in notebook metadata (`metadata.label`)
  - Automatically set from user input when creating new workbooks
  - Displayed in sidebar, workbooks table view, and tab titles
  - Fallback to filename (without .ipynb) when no label is set
  - Makes workbooks feel like actual tools (e.g., "Daily Sales Report" vs "sales_report_v3")
- **Click-to-edit UI**: Edit labels directly in the WorkbookViewer
  - Click the workbook title to enter edit mode
  - Enter to save, Escape to cancel
  - Changes marked as unsaved and persisted on next save
  - Inline editing with visual feedback
- **Backend support**: Rust workbook creation sets initial label
  - New workbooks get label from user input
  - Label preserved through save/load cycle
  - Compatible with existing notebooks (label is optional)
- **Files**:
  - `src-tauri/src/fs.rs` - Label added to notebook metadata on creation
  - `src/components/Sidebar.jsx` - Load and display labels in sidebar
  - `src/components/WorkbooksTableView.jsx` - Display labels in table view
  - `src/components/WorkbookViewer.jsx` - Editable label UI
  - `src/App.jsx` - Load labels for tab titles

**AI-Generated Workbook Creation (Dec 27, 2025)**
- **Fixed "Generate with AI" feature**: Implemented previously non-functional AI workbook generation
  - Uses Claude CLI to generate notebook cells from user description
  - Smart JSON parsing with fallback to extract cells from markdown code blocks
  - Generates complete .ipynb structure with proper metadata
  - Error handling with user-friendly fallback to blank workbook creation
  - Automatically opens generated workbook after creation
  - Location: `src/components/Sidebar.jsx:517-663`
- **Modal UX improvements**:
  - Added loading spinner in "Generate with AI" button during generation
  - Modal stays open and shows progress instead of dismissing immediately
  - All buttons disabled during generation to prevent accidental actions
  - Fixed keyboard focus issue: typing in modal no longer triggers notebook shortcuts
  - Implemented event propagation stopping to keep focus within modal
  - Escape key disabled during generation
  - Location: `src/components/NewWorkbookModal.jsx`

**Resizable and Collapsible Panels (Dec 27, 2025)**
- **ResizablePanel Component**: New reusable component for creating resizable panels
  - Drag handles with visual feedback (blue highlight on hover/drag)
  - Configurable min/max width constraints
  - Automatic persistence of panel sizes to localStorage
  - Support for left or right-side resize handles
- **VS Code-Style Panel Toggles**: Professional panel management in top right corner
  - Icon-only toggle buttons positioned in top right (VS Code standard)
  - Three panels: Primary Sidebar (left), Panel (middle/AI chat), Secondary Sidebar (right/file viewer)
  - Clean, minimal design with white background
  - Active state shown with blue icon color
  - Hover effects with subtle gray background
  - Tooltips showing keyboard shortcuts
- **Standard Keyboard Shortcuts** (matching VS Code):
  - `Cmd+B` (or `Ctrl+B`) - Toggle Primary Sidebar (left sidebar)
  - `Cmd+J` (or `Ctrl+J`) - Toggle Panel (AI chat panel)
  - `Cmd+Shift+B` (or `Ctrl+Shift+B`) - Toggle Secondary Sidebar (file viewer)
- **Panel Management**:
  - Left sidebar: 200-500px range, defaults to 256px
  - AI chat panel: 300-1200px range, defaults to 500px
  - Right panel (file viewer): Toggleable visibility
  - Panel visibility state persisted to localStorage
  - Smooth collapse/expand animations
  - When AI chat is the only visible panel, it takes full width (no resize handle)
  - Maximum flexibility: hide any combination of panels to focus on what matters
- **Smart Layout**:
  - Panels collapse to zero width (completely hidden)
  - File viewer always takes remaining space
  - Resize handles appear on the right side of panels
  - Drag cursor and visual feedback during resize
- **Files**:
  - `src/components/ResizablePanel.jsx` (new) - Reusable resize component
  - `src/App.jsx` - Panel integration, keyboard shortcuts, toolbar
  - `features/todo.md` - Updated to mark feature complete
  - `features/changelog.md` - This entry

**AI Chat: Immediate Thinking Indicator Fix (Dec 27, 2025)**
- **Problem**: Blank gray box for 30+ seconds after sending message - no feedback that Claude was processing
- **Solution**: Added inline "Claude is thinking..." indicator with animated bouncing dots
- **Triggers**: Shows immediately when streaming message has no content/progress yet
- **Impact**: Eliminates user confusion about whether app is working or frozen
- **Location**: src/components/AiChatPanel.jsx:824-843

**AI Chat: Notebook Change Approval System (Dec 27, 2025)**
- **Diff Modal**: Beautiful cell-by-cell comparison when Claude modifies notebooks
- **Color Coding**: Green (added), blue (modified), red (deleted) for clear visibility
- **Approval Flow**: User must approve/reject changes before they're saved
- **Version History**: Automatic snapshots saved to `.workbooks/versions/{notebook}/{timestamp}.ipynb`
- **Manual Revert**: "↶ Revert" button in WorkbookViewer toolbar for easy undo
- **Backend**: 6 new Tauri commands for complete version control
- **Safety**: No unwanted AI changes can be saved without explicit user approval

**AI Chat: Markdown Rendering & Enhanced Progress Indicators (Dec 27, 2025)**
- **Markdown Rendering**: AI responses now display with beautiful, readable formatting
  - **Full GFM Support**: Code blocks, headers, lists, links, tables all render properly
  - **Syntax Highlighting**: Code examples in responses are highlighted for readability
  - **Clean Typography**: Using Tailwind's typography plugin for professional prose styling
  - **User Messages**: Remain plain text (as typed) for simplicity
  - **Implementation**: Added react-markdown and remark-gfm libraries
- **Enhanced Progress Indicators**: Much more visible feedback during AI operations
  - **Prominent Blue Boxes**: Progress events now displayed in colored, bordered containers
  - **Animated Dots**: Each event shows pulsing dot indicator for clear visual feedback
  - **Better Separation**: Progress clearly separated from main response content
  - **Visible Backgrounds**: Blue background and border for high visibility
- **Improved Thinking State**: Better feedback when Claude is processing
  - **"Claude is thinking..." Text**: Clear message alongside bouncing dots
  - **Blue Theme**: Consistent with app color scheme
  - **Proper Timing**: Only shows before assistant response appears
  - **Visual Consistency**: Matches other progress indicators
- **Bug Fixes**:
  - Fixed deprecated `onKeyPress` API → replaced with `onKeyDown`
  - Proper handling of Enter key for sending messages
  - Maintains Shift+Enter for new lines
- **Impact**:
  - Chat is now professional and easy to read
  - Code examples are highlighted and formatted beautifully
  - Users can clearly see when Claude is working and what it's doing
  - No more raw markdown cluttering the interface
  - Polished, production-ready chat experience
- **Files**: src/components/AiChatPanel.jsx, package.json (react-markdown, remark-gfm)
- **Documentation**: features/ai-assistant/done.md, features/ai-assistant/todo.md

### December 2025

**Development: Fixed port conflict preventing dev server from starting (Dec 27, 2025)**
- **Bug Fix**: Resolved critical issue where development server wouldn't start
  - **Root Cause**: Port 1420 was already in use by zombie processes from previous dev server instances
  - **Solution**: Terminate existing processes using `pkill -f "npm run tauri dev" && pkill -f "vite"`
  - **Impact**: Development environment now reliably starts without manual intervention
  - Location: features/todo.md critical issues section

**AI Chat: Smart session naming & rename functionality (Dec 27, 2025)**
- **Chat History Improvements**: Dramatically improved chat session naming and management
  - **Smart Title Generation**: Auto-generates meaningful titles from first user message
    - Removes common prefixes ("can you", "please", "help me", etc.)
    - Capitalizes and truncates intelligently at word boundaries
    - Examples: "can you help debug?" → "Help debug", "Create data pipeline..." → "Create data pipeline..."
  - **Rename Functionality**: Added inline session renaming with edit icon on hover
    - Click edit icon → input field appears
    - Press Enter to save, Escape to cancel, blur to save
    - Updates database with new title and timestamp
  - **Better Metadata Display**: Shows relative timestamps and model info
    - "just now", "5m ago", "2h ago", "3d ago" format
    - Model name displayed alongside time (e.g., "2h ago · Sonnet 4.5")
    - Bold titles for better visual hierarchy
  - **Impact**:
    - Chat history is now easy to browse and search through
    - No more generic "New Chat" titles
    - Users can find specific conversations quickly
    - Cleaner, more professional UI

**AI Chat: Fixed blank responses bug (Dec 27, 2025)**
- **Bug Fix**: Fixed critical issue causing AI assistant to return blank responses
  - **Root Causes**:
    1. For messages requiring approval: Enhanced prompt was only used during plan, not execution
    2. For simple messages: Plan mode response was shown directly (plan mode analyzes but doesn't execute)
  - **Solutions**:
    - **Fix #1**: Added `pendingEnhancedPrompt` state and updated `handleApprove()` to use enhanced prompt
    - **Fix #2**: Changed no-changes path from showing plan response to executing with streaming
    - Both paths now use enhanced prompt with full project context
    - Added streaming event listeners for real-time content updates
    - Proper error handling and cleanup in all execution paths
  - **Impact**:
    - AI responses now include proper content in ALL scenarios (simple questions + approved changes)
    - Claude has consistent context across plan and execution phases
    - Users get meaningful, helpful responses instead of blank messages
    - Project context (notebooks, best practices) preserved throughout conversation
    - Same streaming UX for all responses (progress indicators, real-time updates)
- **Files**: src/components/AiSidebar.jsx
- **Documentation**: features/ai-assistant/done.md

**AI Chat: Auto-start chat session on project open (Dec 27, 2025)**
- **Feature**: Fresh chat sessions automatically created when opening a project
  - **Backend Changes**:
    - Added `project_root` field to `ChatSession` struct and database schema
    - Created `get_or_create_project_session()` function that always creates a new session
    - New Tauri command `get_or_create_project_chat_session` for frontend integration
    - Database migration automatically adds `project_root` column to existing sessions
  - **Frontend Changes**:
    - Modified `loadProjectFromPath()` in App.jsx to auto-create chat session
    - Added `initialSession` prop to AiChatPanel component
    - New useEffect in AiChatPanel watches for initialSession and loads it automatically
    - All new sessions (manual or auto-created) are associated with current project
  - **User Experience**:
    - No need to manually create a chat session when opening a project
    - Every project open starts with a clean slate for conversation
    - Previous chat sessions are preserved in the history sidebar
    - Each session is tagged with the project it was created for (easy to find later)
  - **Technical Details**:
    - Always creates new session with project_root for organization
    - Previous sessions remain in database and accessible via history
    - Default model (Sonnet 4.5) set for new project sessions
- **Files**: src-tauri/src/chat_sessions.rs, src-tauri/src/lib.rs, src/App.jsx, src/components/AiChatPanel.jsx
- **Documentation**: features/ai-assistant/done.md, features/ai-assistant/docs.md

**AI Chat: Improved error handling and UX (Dec 27, 2025)**
- **Bug Fix**: Fixed poor error handling when Claude CLI fails
  - **Issue**: When errors occurred, user would see both a streaming placeholder (with blinking cursor) AND an error message
  - **Root Cause**: Error handler added new error message but never cleaned up the streaming placeholder message
  - **Impact**: Confusing UI with orphaned streaming messages, unclear error states
  - **Solution**:
    - Error handler now finds and replaces the streaming placeholder with error content
    - Removed `isStreaming` flag and set `isError` flag on the message
    - Added user-friendly error message: "I encountered an error: [details]. Please try again or rephrase your request."
    - Fallback logic if streaming message not found (defensive programming)
  - **Fixed Locations**:
    - AiChatPanel.jsx:352-378 - Clean up streaming message on error
    - AiSidebar.jsx:410-436 - Clean up streaming message on error in handleApprove
  - **Result**: Clean error states, no orphaned UI elements, better error messages
- **Files**: src/components/AiChatPanel.jsx, src/components/AiSidebar.jsx

**AI Chat: Fixed duplicate message race condition (Dec 27, 2025)**
- **Bug Fix**: Resolved duplicate messages appearing when Enter is pressed rapidly
  - **Issue**: Same user message would appear 2-3 times in chat history
  - **Root Cause**: Race condition - `sending` state was set after adding message to UI state
  - **Impact**: If user pressed Enter multiple times quickly, `handleSend()` would execute multiple times before `sending` became true
  - **Solution**: Move `setSending(true)` to execute immediately after validation check, before message creation
  - **Fixed Locations**:
    - AiChatPanel.jsx:176 - Set sending state before creating message
    - AiSidebar.jsx:215 - Set sending state before creating message
  - **Result**: Chat now properly prevents duplicate sends even with rapid key presses
- **Files**: src/components/AiChatPanel.jsx, src/components/AiSidebar.jsx

**Workbook Viewer: Fixed cell.source TypeError (Dec 27, 2025)**
- **Bug Fix**: Resolved TypeError when loading notebooks with string-format cell sources
  - **Issue**: `TypeError: cell.source.join is not a function` when cell.source was a string instead of array
  - **Root Cause**: Jupyter notebooks support two formats for cell source - array of strings (older format) or single string (newer format)
  - **Solution**: Created `getCellSourceAsString()` helper function to handle both formats (src/components/WorkbookViewer.jsx:24-29)
  - **Updated Locations**:
    - WorkbookCell initialization (line 179)
    - Run All filtering and execution (lines 1783, 1786)
  - **Impact**: Workbooks now load and execute correctly regardless of notebook format version
- **Files**: src/components/WorkbookViewer.jsx

**AI Assistant: Fixed Invalid Permission Mode (Dec 27, 2025)**
- **Bug Fix**: Corrected invalid Claude CLI permission mode argument
  - **Issue**: CLI was using `--permission-mode all` which is not a valid option
  - **Error**: "error: option '--permission-mode <mode>' argument 'all' is invalid"
  - **Valid Options**: acceptEdits, bypassPermissions, default, delegate, dontAsk, plan
  - **Solution**: Changed to `bypassPermissions` for streaming mode (src-tauri/src/claude_cli.rs:266)
  - **Impact**: AI Assistant streaming functionality now works without errors
  - **Context**: The `run_streaming` function needed a valid permission mode to allow all tools without prompting
- **Files**: src-tauri/src/claude_cli.rs

**AI Assistant: Auto-Open Files During Creation/Editing (Dec 27, 2025)**
- **Feature**: Files automatically open in the UI when Claude creates or edits them
  - **Auto-Detection**: Detects Write and Edit tool events in real-time
  - **Instant Opening**: File opens immediately when Claude starts writing/editing
  - **Type Detection**: Automatically determines file type based on extension (.ipynb, .py, etc.)
  - **Real-Time Viewing**: Watch content appear as Claude creates it
  - **Focused Context**: Opened files automatically become context for further conversation
- **Implementation**:
  - Added `onOpenFile` callback prop to `AiChatPanel` (passed from `App.jsx`)
  - Event listener detects tool_use/tool_result events for Write/Edit tools
  - Extracts file_path from tool input parameters
  - Calls onOpenFile() with path and type to trigger tab opening
- **Benefits**: Seamless workflow, no manual file opening, better understanding of AI actions, natural real-time collaboration

**AI Assistant: Model Selection Persistence & Expanded Model List (Dec 27, 2025)**
- **Feature**: Fixed model selection to persist per-session and show all available models
  - **Expanded Model List**: Main dropdown now shows all Claude models (Sonnet 4.5, Opus 4.5, Opus Plan, Sonnet, Opus, Haiku, Sonnet 1M, Default)
  - **Friendly Names**: Models displayed with clear labels and descriptions (e.g., "Sonnet 4.5 - Latest Sonnet (default)")
  - **Per-Session Persistence**: Each chat session remembers which model was used
  - **Visual Indicators**: Session list displays model name below chat title
  - **Auto-Restore**: When loading a session, model selector automatically updates to match
  - **Simplified UX**: Removed confusing Advanced section with custom model input
- **Backend**:
  - Added `model` field to `ChatSession` struct and database schema
  - Database migration: automatically adds model column to existing sessions table
  - Updated all CRUD operations to handle model field (create, list, get)
  - Modified Tauri commands to accept and persist model parameter
- **Frontend**:
  - Created `MODEL_OPTIONS` array with all available models and metadata
  - Added `getModelDisplayName()` helper for friendly names
  - Updated session creation and loading to save/restore model
  - Enhanced session list UI to show model context
  - Default model: `claude-sonnet-4-5-20250929` (latest Sonnet)
- **Benefits**: All models easily accessible, sessions remember model choice, cleaner UI, better model visibility

**AI Assistant: History Sidebar with Search (Dec 27, 2025)**
- **Feature**: Redesigned chat history as a filterable sidebar
  - **Sliding Sidebar**: Slides in from left with overlay backdrop (320px wide)
  - **Search Filter**: Real-time search input to filter chats by title
  - **Better Chat List**: Shows title + date, highlights active chat, hover to delete
  - **Auto-Close**: Loads chat and closes sidebar on click
  - **Smart States**: Loading, empty, and no-results states
- **Feature**: Prominent "New Chat" button
  - **Always Visible**: Button permanently displayed in header with blue styling
  - **Renamed**: "Sessions" → "History" for clearer purpose
- **Benefits**: Easier browsing, quick search, familiar UX pattern, cleaner interface

**AI Assistant: Migration to Claude Code CLI (Dec 27, 2025)**
- **Feature**: Replaced Claude Agent SDK with Claude Code CLI integration
  - **No API Keys**: Users authenticate via Claude Code CLI, no need to manage API keys in Workbooks
  - **Installation Check**: Automatically detects if Claude Code is installed on startup
  - **Session Persistence**: Uses `--session-id` flag to maintain conversation history per project
  - **Two-Phase Workflow**: Plan mode → Approval → Execute ensures user control
  - **Approval Modal**: New UI shows pending changes with tool-by-tool approval (ClaudeApprovalModal.jsx)
  - **Tool Permissions**: Users can selectively approve Read, Edit, Write, Bash operations
  - **Streaming Support**: Real-time output streaming during execution
  - **Project Sessions**: Each project gets its own Claude session ID stored in `.workbooks/claude_session`
- **Backend**:
  - New `claude_cli.rs` module handles CLI execution (src-tauri/src/claude_cli.rs)
  - Commands: `check_installation()`, `run_plan_mode()`, `run_with_approval()`, `run_streaming()`
  - Six new Tauri commands: `check_claude_cli_installed`, `claude_cli_plan`, `claude_cli_execute`, `claude_cli_stream`, `claude_cli_continue`, `claude_cli_get_session_id`
  - **Cleanup**: Removed old SDK code (`agent.rs`, `/agent/chat` endpoint, SDK dependency)
- **Frontend**:
  - Updated AiSidebar.jsx to use CLI instead of SDK
  - Header changed from "AI Assistant" to "Claude Code"
  - Installation prompt if Claude CLI not detected
  - Shows Claude Code version in footer
- **Benefits**: Better security model, easier user setup, built-in permissions, lower maintenance, no SDK version management

**File System: Auto-Sync, Inline Renaming & Tab Lifecycle (Dec 22, 2025)**
- **Feature**: File list now stays current with external changes
  - **Backend**: Implemented file system watching using `notify` crate (src-tauri/src/watcher.rs)
  - **Debouncing**: 500ms debounce prevents event spam during rapid changes
  - **Filtering**: Automatically ignores .git, .workbooks, node_modules, .venv, __pycache__, etc.
  - **Event Architecture**: Emits 'file-system-changed' events to frontend via Tauri
  - **Auto-Start**: File watcher automatically starts when project opens (src-tauri/src/lib.rs:204-230,256-275)
  - **Frontend Integration**: Sidebar listens for file system events and auto-refreshes (src/components/Sidebar.jsx:237-258)
- **Feature**: VS Code-style inline renaming
  - **UI**: Right-click → Rename shows inline input field directly in tree
  - **Smart Selection**: Auto-selects filename without extension for quick editing
  - **Keyboard**: Enter confirms, Escape cancels, click-away cancels
  - **Universal**: Works in file tree, nested folders, and search results
  - **Visual**: Blue border highlight for active rename input
- **Feature**: Tab filename propagation
  - **Auto-Update**: Tab titles automatically update when files are renamed
  - **Path Tracking**: Tab paths update to reflect new location
  - **No Refresh Needed**: Changes propagate immediately via callbacks
- **Feature**: Deleted file persistence in tabs
  - **Keep Tabs Open**: Deleted files stay open in tabs (don't auto-close)
  - **Visual Indicators**: Red tab background, "(deleted)" suffix, warning banner
  - **Content Preservation**: File content remains in memory, editable
  - **One-Click Restore**: "Save to Restore" button recreates the file
  - **Graceful Recovery**: Tab returns to normal state after restoration
- **Implementation Files**:
  - src-tauri/Cargo.toml (added notify and notify-debouncer-mini dependencies)
  - src-tauri/src/watcher.rs (new file - file watching implementation)
  - src-tauri/src/lib.rs:14,204-230,256-275,686-712 (watcher integration, file lifecycle)
  - src/components/Sidebar.jsx:43-90,162-197,209-221,229,243,477-479,561-595,881-912 (inline renaming, rename propagation)
  - src/App.jsx:686-712,912,980-992 (tab lifecycle management)
  - src/components/TabBar.jsx:19-32 (deleted file visual indicators)
  - src/components/FileViewer.jsx:6,12,151-189,513-531 (deleted file handling, restore functionality)

**AI Assistant: Request Cancellation (Dec 22, 2025)**
- **Feature**: Added ability to cancel long-running AI agent requests
  - **Backend**: Implemented cancellation channel using `tokio::oneshot` in agent.rs
  - **State Tracking**: Added `active_agent_requests` HashMap to AppState for managing cancellation handles
  - **Race Condition**: Used `tokio::select!` to race between HTTP request and cancellation signal
  - **UI**: Added red "Cancel" button that appears next to loading indicator during active requests
  - **Cleanup**: Properly removes active request tracking on completion, error, or cancellation
  - **User Feedback**: Shows "Request cancelled by user" message when cancelled
- **Implementation Files**:
  - src-tauri/src/agent.rs:18,47-54,66-75,94-160,184-199 (cancellation logic)
  - src-tauri/src/lib.rs:36,1717,1810 (AppState and command registration)
  - src/components/AiSidebar.jsx:145-165,392-398 (cancel button and handler)

**AI Assistant Fixes (Dec 22, 2025)**
- **Fixed**: Agent communication errors resolved
  - Fixed hardcoded port 8765 → Now uses dynamic engine server port (src-tauri/src/agent.rs:16,40,99-102)
  - Fixed type error when concatenating list to string → Now handles content blocks properly (src-tauri/engine_server.py:1094-1121)
  - Added engine server startup check in AiSidebar.jsx before sending messages (line 88)
  - Improved error handling with 300s timeout and detailed logging
  - Added comprehensive debug logging for stream events and chunk processing

**AI Assistant Integration (Dec 21, 2025)**
- **Feature**: Claude Agent SDK integration for inline chat assistance
  - **Backend**: Python SDK installed in engine environment via UV
  - **Endpoint**: `/agent/chat` in engine_server.py with SSE streaming
  - **Storage**: SQLite database at `~/.workbooks/chat_sessions.db` for chat history
  - **UI Component**: AiSidebar.jsx with session management and real-time chat
  - **Integration**: Always-visible sidebar that prompts to enable AI when disabled
  - **Tauri Commands**:
    - `create_chat_session`, `list_chat_sessions`, `get_chat_session`
    - `delete_chat_session`, `add_message_to_session`
    - `send_agent_message` - Sends to agent and streams response
  - **Toggle UI**: Floating action button in bottom-right with status indicator
  - **Architecture**: Reuses existing engine_server.py, no additional processes needed
  - **Security**: API key stored in system keychain, never logged
- **Implementation Files**:
  - src-tauri/engine_server.py:1028-1111 (agent endpoint)
  - src-tauri/src/chat_sessions.rs (SQLite persistence)
  - src-tauri/src/agent.rs (HTTP agent communication)
  - src/components/AiSidebar.jsx (chat UI)
  - src/App.jsx:12,31-32,57-64,909-941 (integration)
- **Documentation**: features/ai-assistant/ (docs.md, todo.md, done.md)

**Fixed: Tray Menu Behavior (Dec 21, 2025)**
- **Bug Fix #1**: Tray menu items now work correctly when all windows are closed/hidden
  - **Root Cause**: Hidden windows are completely removed from Tauri's window HashMap
    - `get_webview_window("main")` returns `None` for hidden windows
    - `webview_windows()` iterator shows empty HashMap when all windows hidden
  - **Solution**: Two-path handling strategy
    - **Path 1 (Window Exists)**: Show window and emit navigation event
    - **Path 2 (No Window)**: Create new window with URL parameters
  - **Window Creation**: Added `create_main_window()` helper (src-tauri/src/lib.rs:1337-1374)
    - Accepts optional `view` parameter (e.g., "global-schedules", "global-runs")
    - Builds URL: `index.html?view={view}`
    - Configures close handler to hide instead of quit
  - **URL-based Navigation**: App.jsx reads query parameters on mount (lines 30-48)
    - `?view=` parameter → Navigate to specific view
    - `?project=` parameter → Load specific project
  - **React Component Keys**: Force remounting when switching views (App.jsx:727, 752)
    - `key="global-schedules"` for schedules view
    - `key="global-runs"` for runs view
    - Prevents data from getting stale when switching between similar views
  - **Fixed Actions**:
    - Recent project menu items → Opens project in new window if needed
    - Create/Open Project menu items → Shows appropriate view
    - View Runs/Scheduler menu items → Creates window or navigates existing
    - Install MCP menu item → Shows placeholder
- **Bug Fix #2**: Opening tray menu no longer resets window to Action Window
  - **Root Cause**: Tray icon click handler was triggering on menu open (macOS behavior)
  - **Solution**: Removed tray icon click handler entirely (src-tauri/src/lib.rs:1295-1300)
    - On macOS, clicking tray icon opens menu (no separate click action needed)
    - All functionality accessible through menu items
    - Menu can be browsed without affecting window state
    - Only selecting menu items triggers actions
  - **Result**: Standard macOS tray menu behavior - browse freely, act on selection
- **Implementation Files**:
  - src-tauri/src/lib.rs:1295-1458 (window management helpers, tray menu handlers)
  - src/App.jsx:30-48 (URL parameter parsing), 169-212 (tray event handlers), 727, 752 (React keys)
- **Result**: Full tray functionality with correct window lifecycle management

**Action Window - Central Launcher and Entry Point (Dec 20, 2025)**
- **New Entry Point**: Action Window now serves as the main entry point to Workbooks
  - Clean, centered launcher UI with Workbooks branding
  - Appears on app startup when no project is loaded
  - Replaces the previous "Welcome" screen with richer functionality
- **Recent Projects**: Shows 3 most recently opened projects
  - Click to open a project (or focus if already open)
  - Displays project name and path
  - Empty state when no recent projects exist
- **Quick Actions**: Fast access to common operations
  - Create Project - Opens project creation flow
  - Open Project - Opens native folder picker to select existing project
  - **View All Runs** - Opens global run history across all projects ✅ IMPLEMENTED
  - **View All Schedules** - Opens global scheduler view across all projects ✅ IMPLEMENTED
  - Install MCP - Placeholder for MCP management UI (coming soon)
- **Global Views**: Fully functional cross-project views
  - View All Schedules shows all scheduled workbooks from all projects
  - View All Runs shows execution history from all projects
  - Both views reuse ScheduleTab component with enhanced global mode support
  - Tray menu items now navigate to these views instead of showing placeholders
- **Integrated with Tray**: Works seamlessly with system tray menu
  - Tray menu items navigate through the Action Window
  - Recent projects list shared between tray and Action Window
  - Create/Open project actions accessible from both
  - **FIXED:** Tray events now properly received when window is hidden
- **Clean Design**: Follows Workbooks style guide
  - Grayscale palette with blue accents
  - Minimal, professional aesthetic
  - Smooth transitions and hover states
  - Proper loading and empty states
- **Backend**: New `get_recent_projects` Tauri command
  - Returns recent projects from ~/.workbooks/recent_projects.json
  - Integrated with existing recent_projects.rs module
  - Backend: `src-tauri/src/lib.rs` - Added get_recent_projects command (line 273)
  - Frontend: `src/components/ActionWindow.jsx` - New launcher component
  - Frontend: `src/App.jsx` - Integrated Action Window routing

**Pagination and Filtering for Recent Runs (Dec 20, 2025)**
- **Pagination Controls**: Added full pagination support for the Recent Runs tab
  - Page size selector: 10, 20, 50, or 100 runs per page
  - Smart page navigation showing first, last, current, and adjacent pages
  - Shows "X to Y of Z runs" counter
  - Previous/Next buttons with disabled state on boundaries
  - Automatically resets to page 1 when filtering or changing page size
- **Date Range Filtering**: Filter runs by start and end date
  - Start date and end date inputs with native date pickers
  - "Clear" button to reset date filters
  - Empty state message updates based on filter status
  - Filters reset pagination to page 1 for better UX
- **Backend Pagination**: New database queries with efficient pagination
  - `list_runs_paginated`: Returns runs with LIMIT and OFFSET
  - `count_runs`: Returns total count for pagination calculation
  - Optional start_time and end_time filtering in both queries
  - Backward compatible - old `list_runs` still works
- **Auto-refresh Compatible**: Pagination state preserved during 3-second auto-refresh
  - Backend: `src-tauri/src/scheduler.rs` - Added pagination queries, date filtering
  - Backend: `src-tauri/src/lib.rs` - New Tauri commands: `list_runs_paginated`, `count_runs`
  - Frontend: `src/components/ScheduleTab.jsx` - Pagination UI, date filters, state management

**Fixed "Run Now" Scheduler Functionality (Dec 20, 2025)**
- **Database Migration**: Added automatic schema migration for the `metadata` column in runs table
  - Fixed "no such column: metadata" error that prevented "Run Now" from working
  - Database automatically migrates on app startup using `pragma_table_info`
  - No manual intervention required - existing databases are upgraded automatically
- **Enhanced Logging**: Added comprehensive logging throughout scheduler execution
  - Logs now show each step: venv setup, engine start, cell execution, cleanup
  - Better error messages with full context for debugging
  - Failed runs are now properly marked as "failed" in the database
- **Improved Error Handling**: Background execution errors are now caught and recorded
  - Run status correctly updates even when execution fails
  - Error messages are stored in the database for viewing in Recent Runs
  - Backend: `src-tauri/src/scheduler.rs` - Added migration logic, logging, error handling

**Enhanced Tray Menu with Recent Projects & Navigation (Dec 20, 2025)**
- **Rich Tray Menu**: Expanded system tray with quick access to projects and features
  - Recent Projects section (max 3) - click to open or focus existing window
  - "Create Project..." and "Open Project..." menu items
  - "View Runs" and "View Scheduler" navigation items
  - "Install MCP..." placeholder for future MCP management
  - Automatic recent projects tracking in `~/.workbooks/recent_projects.json`
  - Backend: `src-tauri/src/recent_projects.rs` - Recent projects storage and retrieval
  - Backend: `src-tauri/src/lib.rs` - Dynamic tray menu construction, event emission
  - Frontend: `src/App.jsx` - Tray event listeners, window management, navigation
  - All tray events show/focus window if hidden, navigate appropriately
  - Documentation: `features/tray/docs.md` - Full design and implementation guide

**Execution Insights for Scheduler (Dec 20, 2025)**
- **Enhanced Run History**: Recent Runs now shows detailed execution metadata for each workbook run
  - Expandable run rows with click-to-expand/collapse functionality
  - Execution summary cards showing cells executed, succeeded, and failed
  - Final cell outputs display (last 3 outputs) for quick preview
  - Full error messages and tracebacks in expandable view
  - New "Cells" column showing "X/Y" (succeeded/executed) summary
  - Arrow indicators (▶/▼) show expandable state
  - Metadata stored as JSON in runs table (not in git)
  - Backend: `src-tauri/src/scheduler.rs` - Added metadata column, ExecutionMetadata struct, metadata extraction
  - Frontend: `src/components/ScheduleTab.jsx` - RunRow component, expandable UI, metadata parsing
  - Future: Variable inspection, report file saving

### December 2024

**System Tray for Background Scheduling (Dec 20, 2024)**
- **System Tray Implementation**: App now runs in menu bar/system tray for reliable scheduled execution
  - Added `tray-icon` feature to Tauri
  - System tray menu with "Open Workbooks", "Scheduler: Running", and "Quit Workbooks" options
  - Closing window hides the app instead of quitting - scheduler continues running in background
  - App only quits when "Quit Workbooks" is selected from tray menu
  - Solves the core issue: schedules now work even when the main window is closed
  - Familiar UX pattern similar to Docker Desktop, Ollama, and other menu bar apps
  - Files: `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`

- **Global Schedule Manager**: View and manage schedules across all projects from one place
  - Added "All Projects" toggle button in Schedule tab header
  - Switches between "Current Project" view and global "All Projects" view
  - "Project" column appears when viewing all projects
  - Works for both scheduled workbooks and run history
  - Enables centralized management of automated data pipelines
  - Files: `src/components/ScheduleTab.jsx`

**Code Preprocessing for Directory Persistence (Dec 20, 2024)**
- **Automatic `!cd` to `%cd` Conversion**: Shell directory changes now persist across cells
  - Added `preprocess_code()` function to engine_server.py
  - Automatically converts `!cd` commands to `%cd` magic before execution
  - Fixes common issue where `!cd some/dir` doesn't persist to next cell
  - Applied to all execution endpoints: `/execute`, `/execute_stream`, `/execute-all`
  - Transparent to users - works without requiring knowledge of IPython magic commands
  - Other shell commands (e.g., `!ls`, `!pwd`) remain unchanged
  - Files: `src-tauri/engine_server.py`

**CLI Implementation: `workbooks run` and `workbooks schedule` (Dec 19, 2024)**
- **Multi-Binary Cargo Setup**: Configured project to build separate CLI and GUI binaries
  - Added `[[bin]]` definitions for `workbooks` (CLI) and `workbooks-gui` (GUI)
  - Made core modules public: `python`, `project`, `engine_http`, `scheduler`
  - Shared library code accessible to both binaries via `workbooks_lib`
  - Files: `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `src-tauri/src/cli.rs`

- **`workbooks run` Command**: Execute notebooks from the command line
  - Parses and executes `.ipynb` files with automatic project detection
  - Walks up directory tree to find `.workbooks` directory for project root
  - Falls back to "basic mode" if no Workbooks project found
  - Automatically ensures Python venv and syncs dependencies
  - Starts engine server and executes all cells via HTTP API
  - Displays execution results, outputs, and errors in terminal
  - Shows summary with cell counts and success/failure status
  - Cleanly shuts down engine after execution
  - Usage: `workbooks run path/to/notebook.ipynb`
  - Optional: `workbooks run notebook.ipynb --project /path/to/project`
  - Files: `src-tauri/src/cli.rs`, `src-tauri/src/engine_http.rs`

- **`workbooks schedule` Commands**: Manage scheduled workbook execution
  - `workbooks schedule add`: Schedule a workbook with cron expression or presets
    - Supports `--cron "0 9 * * *"` for custom schedules
    - Presets: `--daily`, `--hourly`, `--weekly`
    - Stores schedules in SQLite via SchedulerManager
    - Displays confirmation with schedule details and next run time
  - `workbooks schedule list`: View all scheduled workbooks
    - Shows ID, workbook path, project, cron expression, enabled status
    - Displays next run time for each schedule
  - `workbooks schedule remove <id>`: Delete a schedule by ID
  - Files: `src-tauri/src/cli.rs`, `src-tauri/src/scheduler.rs`

- **Engine HTTP Extensions**: Added execute-all endpoint support
  - Added `execute_all_http()` function to call `/engine/execute-all` endpoint
  - New types: `Cell`, `CellExecutionResult`, `ExecuteAllResponse`
  - Enables batch execution of all notebook cells from CLI
  - Files: `src-tauri/src/engine_http.rs`

**Files Section UX Improvements (Dec 19, 2024)**
- **Breadcrumb Navigation**: Added VS Code-style breadcrumb navigation to FileViewer
  - Shows full file path with folder hierarchy instead of just filename
  - Path separator (`/`) between folders
  - Last item (filename) highlighted with bold text
  - Handles text overflow with ellipsis
  - Works in both regular file editor and image viewer headers
  - Graceful fallback to simple filename when project root not available
  - Files: `src/components/FileViewer.jsx`, `src/App.jsx`

- **Focus Retention Fix**: Fixed input focus loss for file/folder creation
  - "+ New File" button now properly retains input focus when clicked
  - "+ New Folder" button now properly retains input focus when clicked
  - Implemented using refs and useEffect for reliable focus management
  - Prevents frustrating focus loss during file creation workflow
  - Files: `src/components/Sidebar.jsx`

**Critical Bug Fixes (Dec 19, 2024)**
- **Cell Movement UI Fix**: Fixed React rendering issue with cell reordering
  - Cells now properly update in the UI when moved up or down
  - Added stable, unique IDs for each cell in metadata
  - Changed from index-based keys to ID-based keys for proper React reconciliation
  - Cells are assigned unique IDs on creation and when loading existing notebooks
  - Files: `src/components/WorkbookViewer.jsx`

- **Markdown Image Display**: Added environment variable support in markdown images
  - Supports `$WORKBOOKS_PROJECT_FOLDER` and `${WORKBOOKS_PROJECT_FOLDER}` in image paths
  - Example: `![plot]($WORKBOOKS_PROJECT_FOLDER/images/plot.png)`
  - Automatically replaces variable with actual project root path
  - Works with relative paths, absolute paths, and HTTP/HTTPS URLs
  - Files: `src/components/WorkbookViewer.jsx`

- **Recursive File Search**: Enhanced Files section with subfolder support
  - Search now works recursively through all subfolders
  - Debounced search (300ms) for better performance
  - Shows file count in search results
  - Displays file path in search results for context
  - Flat list view in search mode shows all matching files with their locations
  - Tree view preserved when not searching
  - Files: `src/components/Sidebar.jsx`

**Native macOS Menu Bar (Dec 19, 2024)**
- **File Menu Fix**: Resolved missing File menu on macOS
  - Added explicit app menu ("workbooks") as first submenu to satisfy macOS requirements
  - File menu now appears correctly between "workbooks" and "Edit" menus
  - Fixed Tauri v2 macOS-specific menu rendering issue
- **New Menu Items**: Enhanced File menu with common actions
  - "New Workbook" (Cmd+N) - Quick workbook creation
  - "Open Project..." (Cmd+O) - Open existing projects
  - "Open Project in New Window..." (Cmd+Shift+O) - Multi-window support
  - "About workbooks" - About dialog (in app menu)
- **Complete Menu Structure**: Professional native menu bar
  - **workbooks** menu: About, Quit
  - **File** menu: New Workbook, Open Project, Open in New Window
  - **Edit** menu: Undo, Redo, Cut, Copy, Paste, Select All
  - **View** menu: Show Runtime Logs (Cmd+Shift+L), Open Logs Folder
  - **Window** menu: Minimize, Maximize, Close Window
- Files: `src-tauri/src/lib.rs` (menu builder, event handlers)

**Files Section Enhancements (Dec 18, 2024)**
- **Notebooks Folder Visibility**: Notebooks folder now appears in FILES section
  - Allows direct access to notebooks from file tree
  - .ipynb files shown when expanding notebooks folder
  - Workbooks can be opened from any location, not just Workbooks section
  - Simplifies file navigation and organization
- **Folder Drag-and-Drop Support**: Complete folder upload capability
  - Drag entire folders into Workbooks to copy them to project
  - Recursive folder copying preserves all subdirectories and files
  - Automatic detection of files vs directories using `stat()`
  - Backend: New `copy_folder_recursively()` and `save_dropped_folder()` functions
  - Frontend: Enhanced drop handler checks file type before processing
  - Works seamlessly with existing file drop system
- **Flexible Notebook Access**: Open notebooks from anywhere in file tree
  - No longer limited to just the Workbooks section
  - Can organize notebooks in custom folder structures
  - Still auto-saves new notebooks to `/notebooks` by default
- **Improved File Tree Filtering**:
  - Removed overly aggressive .ipynb filtering
  - Shows all files within folders including notebooks
  - Better reflects actual project structure
- Files: `src/components/Sidebar.jsx` (updated filtering), `src/App.jsx` (folder drop), `src-tauri/src/fs.rs` (recursive copy), `src-tauri/src/lib.rs` (new command)

**Enhanced Markdown Rendering (Dec 18, 2024)**
- **GitHub Flavored Markdown (GFM) Support**: Full remark-gfm plugin integration
  - Tables with custom styling, sorting, and hover effects
  - Strikethrough text support (~~text~~)
  - Task lists with checkboxes (- [ ] and - [x])
  - Autolinks for URLs and email addresses
- **Mathematical Expressions**: LaTeX math rendering with KaTeX
  - Inline math using $...$ syntax
  - Display math using $$...$$ syntax
  - Full KaTeX CSS integration for proper rendering
- **Rich Text Formatting**: Enhanced typography and styling
  - Bold, italic, and strikethrough text
  - Headers (h1-h6) with custom bottom borders
  - Code blocks with syntax highlighting (via react-syntax-highlighter)
  - Inline code with gray background styling
  - Blockquotes with left border accent
  - Ordered and unordered lists with proper spacing
- **Image Support**: Complete local and remote image handling
  - Remote images from URLs (http/https)
  - Local images via relative paths (e.g., `./images/plot.png`)
  - Local images via absolute paths
  - Automatic conversion to Tauri asset protocol for security
  - Error handling with fallback "Image not found" message
  - Responsive sizing with rounded corners and margins
- **Link Handling**: Smart link routing and styling
  - External links open in new tab with `rel="noopener noreferrer"`
  - Local file links detected (ready for future integration)
  - Custom styling for different link types
  - Blue link color with hover effects
- **Custom Styling**: Tailwind Typography integration
  - Prose classes for clean, readable text
  - Custom table borders and hover effects
  - Proper spacing and typography hierarchy
  - Responsive layout for all content types
- **HTML Support**: Raw HTML rendering via rehype-raw plugin
  - Allows embedded HTML in markdown cells
  - Useful for custom layouts and widgets
- **Dependencies Added**:
  - remark-gfm, remark-math, rehype-katex, rehype-raw
  - KaTeX CSS loaded via CDN in index.html
- **Full Persistence**: All markdown content saves to .ipynb files and renders correctly on reload
- Files: `src/components/WorkbookViewer.jsx` (enhanced), `index.html` (KaTeX CSS), `package.json` (new deps)

**Workbook Execution Enhancements (Dec 18, 2024)**
- **Execution Metadata Tracking**: Cell-level performance metrics
  - Last run timestamp stored in cell metadata
  - Execution duration tracked and displayed
  - Duration shown below execution count ([3] 0.25s)
  - Metadata persisted in notebook file for history
- **Cell Execution Status Indicators**: Visual feedback system
  - Error indicator (✗ symbol) with red highlighting on failed cells
  - Running indicator with blue text and live timer
  - Execution count display matching Jupyter style [3]
  - Duration displayed after completion
- **Execution Queue Controls**: Batch cell execution
  - "Run All Above" button to execute cells above selected
  - "Run All Below" button to execute cells below selected
  - Enhanced "Run All" with metadata tracking
  - Queue progress tracking with cell highlighting
- **Enhanced DataFrame Rendering**: Production-grade table styling
  - Sticky headers that stay visible when scrolling
  - Max height (600px) with scroll for large DataFrames
  - Cleaner borders (bottom-only instead of full grid)
  - Gradient headers with subtle shadows
  - Improved hover effects with smooth transitions
  - Tabular numeric formatting for better number alignment
  - Sticky left column for row indices
- **Image Lightbox/Zoom Feature**: Click-to-zoom functionality
  - Click any PNG/JPEG image to view full-size
  - Dark overlay with centered image
  - Close button and click-outside-to-close behavior
  - Hover effects on thumbnails (cursor change, opacity)
  - Supports images up to 90vh height

**Workbook UI Polish (Dec 18, 2024)**
- **Cell Visual Improvements**: Complete redesign of cell appearance
  - Added clear borders with rounded corners and hover states
  - Improved selection states with blue borders and subtle backgrounds
  - Better execution indicator styling [1], [ ]
  - Tighter spacing between cells for better organization
- **DataFrame Output Styling**: Professional table rendering
  - Zebra striping (alternating row colors)
  - Bold headers with gray backgrounds
  - Proper borders on all cells
  - Hover effects on rows (blue highlight)
  - Better spacing and typography
- **Output Area Enhancements**: Improved all output types
  - Stream outputs with rounded borders and subtle backgrounds
  - Error outputs with red tint for visibility
  - Images with white backgrounds and padding
  - Plain text with proper monospace styling
- **Toolbar Refinements**: Better organization and visual hierarchy
  - Logical grouping (Execution / Kernel / Add Cells)
  - Icons added to all buttons (▶, ⏹, 🔄, 🗙)
  - Visual separators between groups
  - Improved spacing and button styling
- **Monaco Editor Polish**: Cleaner code editing experience
  - Added vertical padding (8px top/bottom)
  - Border around editor container
  - Removed unnecessary UI elements (glyph margin, folding)
  - Consistent with STYLE_GUIDE.md aesthetic

**File Management Feature Enhancements (Dec 18, 2024)**
- **Image Viewer**: Added full image viewing support for PNG, JPG, SVG, GIF, WebP, BMP, ICO
  - Zoom controls (25% - 400%)
  - Reset zoom button
  - Clean, centered display with controls
- **CSV Preview**: Implemented interactive table viewer for CSV files
  - Sortable columns (click header to sort ascending/descending)
  - Automatic numeric vs string detection for sorting
  - Row and column count display
  - Toggle between table view and raw CSV editor
  - Performance optimized (displays first 1000 rows)
- **JSON Tree Viewer**: Built collapsible tree structure for JSON files
  - Expandable/collapsible nodes
  - Type-based syntax highlighting (strings, numbers, booleans, null)
  - Shows object/array size previews when collapsed
  - Auto-expands first 2 levels
  - Toggle between tree view and raw JSON editor
- **File Search**: Added real-time search/filter in Files section
  - Search by filename
  - Live filtering as you type
  - Clear "no matches" messaging
- **File Creation**: Implemented create new file and folder functionality
  - "+ File" and "+ Folder" buttons in Files section
  - Inline creation forms with validation
  - Auto-refresh file list after creation
  - Backend Tauri commands: `create_new_file()`, `create_new_folder()`
- **Visual Drop Zone**: Confirmed existing drop zone indicator working
  - Blue dashed border overlay when dragging files
  - Clear messaging about file destinations
- Updated features/files documentation (done.md, todo.md) with completed items

**Secrets Output Warning System (Dec 18, 2024)**
- Implemented proactive warning system to prevent secret leakage in workbook outputs
- Added `scan_outputs_for_secrets` Tauri command to detect secrets in cell outputs
- **Proactive detection**: Automatically scans outputs after every cell execution
  - Save button changes to amber "⚠ Save" when secrets detected
  - Tooltip warns: "Secrets detected in outputs - click to review"
  - Visual feedback BEFORE user attempts to save
- Created SecretsWarningModal component with professional, clean design
  - Warning icon and clear messaging about security risks
  - Shows list of affected cell indices (e.g., "Cell [1]", "Cell [3]")
  - Three action options: "Clear and Save", "Go Back and Fix", "Dangerously Save Anyway"
  - Two-step confirmation for dangerous save action
  - Follows app style guide (amber warning colors, proper typography)
- Integrated scanning into WorkbookViewer workflow
  - Scans on cell execution, not just on save (proactive vs reactive)
  - Blocks save if secrets detected until user makes a choice
  - Prevents accidental exposure of secrets in Git commits or shared notebooks
- Backend scanning logic checks all cell outputs against stored secret values
- Updated secrets documentation (todo.md, done.md) to reflect completion

**UI Style Guide & Secrets Manager Redesign (Dec 18, 2024)**
- Created comprehensive `STYLE_GUIDE.md` defining Workbooks's design system
- Redesigned Touch ID authentication gate to match app aesthetic
  - Removed heavy gradients and shadows
  - Changed from purple gradient to clean gray background
  - Replaced gradient button with standard blue primary button
  - Centered layout with proper spacing and typography
  - Fixed authentication gate header to use Tailwind (removed old CSS classes)
- Complete redesign of AddSecretDialog component
  - Converted from inline CSS to Tailwind utility classes
  - Removed emoji buttons (🔐, 👁️) replaced with text ("Show", "Hide", "Authenticate")
  - Changed yellow warning background to clean blue info box
  - Dialog now follows style guide overlay and card pattern
  - Improved form inputs with proper focus states
- Secrets Manager main interface redesign
  - Removed all inline `<style>` block (280+ lines of custom CSS)
  - Converted entire component to Tailwind utilities
  - Table action buttons changed from emoji (✏️, 🗑️) to text ("Edit", "Delete")
  - Consistent button styling (primary, secondary, danger patterns)
  - Improved table styling with proper hover states
  - Clean, professional aesthetic throughout
  - Removed emoji from header titles (both auth and main view)
- Secrets tab improvements
  - Changed tab name from "🔐 Secrets" to just "Secrets" (removed emoji)
  - Clean, professional appearance in tab bar
- Sidebar emoji removal (complete cleanup)
  - Removed emojis from all section headers (Workbooks, Secrets, Schedule, Files)
  - Removed file type emojis (🐍 for .py, 📝 for .md, ⚙️ for config files, etc.)
  - Removed workbook list item emoji (📓)
  - Removed Project Settings button emoji (⚙️)
  - Kept functional arrows (▶/▼) for folder expand/collapse
  - Clean, text-only interface throughout sidebar
- Tab bar cleanup
  - Removed all file type emojis from tabs (📓, 🐍, 📝, ⚙️, 📄)
  - Removed autosave toggle (non-functional UI element)
  - Tab bar now shows only when tabs are open
  - Clean, minimal tab display with just filenames
- WorkbookViewer toolbar cleanup
  - Removed secrets count badge (🔐 with count)
  - Removed Admin Mode toggle button (🔒/🔓)
  - Removed all secrets-related state and functions (loadSecretsCount, toggleAdminMode)
  - Secrets in cell output remain redacted (security feature preserved)
  - Cleaner, less cluttered toolbar
- Updated CLAUDE.md with UI Design section referencing style guide
- Style guide includes: color palette, typography, spacing, component patterns, layouts, accessibility
- All future UI work must follow the approved design patterns
- Files: `STYLE_GUIDE.md` (new), `src/components/SecretsManager.jsx` (redesigned), `src/components/Sidebar.jsx` (cleaned), `src/components/TabBar.jsx` (cleaned), `src/components/WorkbookViewer.jsx` (cleaned), `src/App.jsx` (updated), `CLAUDE.md` (updated)

**Secrets Management (Dec 18, 2024)**
- Complete secrets management system with encryption, keychain integration, and UI
- AES-256-GCM encryption with per-project keys stored in system keychain
- SQLite database for encrypted secrets storage (`.workbooks/secrets.db`)
- Full CRUD interface via SecretsManager component
- Automatic injection of secrets as environment variables into workbook kernels
- Sidebar integration with live count and quick access
- WorkbookViewer indicator badge showing active secrets
- Import from .env files functionality
- 7 new Tauri commands: add_secret, get_secret, list_secrets, update_secret, delete_secret, get_all_secrets, import_secrets_from_env
- Real-time updates via event system
- Files: `src-tauri/src/secrets.rs` (new), `src/components/SecretsManager.jsx` (new), updated engine_http.rs, lib.rs, Sidebar.jsx, WorkbookViewer.jsx, App.jsx
- See `features/secrets/done.md` for full details

**Workbook Execution System (~85% MVP Complete)**
- Full-featured WorkbookViewer with Monaco editor
- Streaming output via Server-Sent Events for real-time feedback
- Rich output rendering (PNG, JPEG, SVG, HTML, DataFrames)
- Engine lifecycle management (start, stop, interrupt, restart)
- Kernel status indicator with real-time updates
- Jupyter-style keyboard shortcuts (DD delete, A/B insert, M/Y type change, arrows)
- Auto-save system (3s interval + on-blur + on-run) with toggle
- Cell operations (add, delete, move, type change, clear output)

**File Management**
- Complete file operations backend (read, write, rename, delete, duplicate)
- FileViewer with Monaco editor and multi-language support
- Markdown preview mode with rendered output
- Context menu for file operations (rename, delete, duplicate)
- Input dialog for rename/duplicate flows
- Drag-and-drop file upload (.ipynb → /notebooks, others → root)
- WORKBOOKS_PROJECT_FOLDER environment variable injection into kernels

**Navigation & UI**
- Tab-based navigation system for multiple open files
- TabBar component with autosave toggle
- Support for workbook and file tabs
- Active tab highlighting and close functionality

**Sidebar**
- Multi-section sidebar layout (Workbooks, Secrets, Schedule, Files, Settings)
- Workbooks section with recent-use ordering (last 20 tracked)
- Workbooks table view modal with metadata
- Files section with tree view and file type icons
- Secrets and Schedule section placeholders
- Project Settings gear icon

**Project & Python Management**
- Project creation with uv integration
- Virtual environment management (centralized at ~/.workbooks/venvs/)
- Python package installation via uv
- Dependency syncing from pyproject.toml
- HTTP engine server (FastAPI) for Jupyter kernel management
- Per-workbook engine isolation

**Backend Infrastructure**
- Tauri app scaffolding (Rust + React 19)
- Python/uv integration with automatic installation
- File system operations (list, read, write, rename, delete)
- Engine HTTP server for kernel lifecycle
- Project management (create, open, load)

### Earlier Work

**Initial Setup**
- Tauri + React 19 + Vite build system
- Welcome screen and create project wizard
- Basic application state management
- File explorer with collapsible tree view

## What's Next

See `features/todo.md` for the high-level roadmap and individual feature areas for detailed implementation plans.

**Priority Features:**
1. Network status indicators and offline behavior (see features/network/)
2. Tab-based navigation for management views (see features/navigation/)
3. Secrets management system (see features/secrets/)
4. Schedule system with cron scheduling (see features/schedule/)
5. Project settings UI (see features/project-settings/)
6. State management system (see features/state/) - Major future feature
