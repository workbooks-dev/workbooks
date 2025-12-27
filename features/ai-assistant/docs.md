# AI Assistant

## Overview

The AI Assistant provides inline chat capabilities powered by the Claude Agent SDK. It allows users to get help with their workbooks, debug code, automate tasks, and interact with their project using natural language.

## Architecture

### Backend (Rust + Claude Code CLI)
- **Claude Code CLI** integration for all AI operations
- **Direct execution** with pre-approved tools (Read, Write, Edit, Bash, Glob, Grep, Task)
- **Streaming responses** via Tauri events with real-time progress indicators
- **Session management** using Claude Code's built-in session resumption

### Frontend (React)
- **AiChatPanel.jsx** - Main chat interface component in the primary panel
- **Always visible** - AI chat is the primary interface
- **Split-view layout** - When files are opened, AI chat on left, file viewer on right
- **Focused file context** - Open files automatically become context for Claude
- **Chat sessions** persist across app restarts

### Data Storage (SQLite)
- **Chat sessions database** at `~/.workbooks/chat_sessions.db`
- **Tables:**
  - `sessions` - Session metadata (id, title, created_at, updated_at, model, project_root)
  - `messages` - Chat messages (session_id, role, content, timestamp)
- **Session-Project Association:**
  - Each session can be associated with a specific project via `project_root`
  - When opening a project, a fresh new chat session is automatically created
  - Previous sessions for that project remain accessible in the history sidebar
  - Enables organized per-project chat history and easy access to past conversations

## Project Context Awareness

The AI Assistant is deeply integrated with Workbooks and understands the project structure:

### Automatic Context Injection
Every prompt sent to Claude includes comprehensive project context:
- **Project name** and root path
- **Purpose statement**: "Building automations with Jupyter notebooks"
- **List of existing notebooks** with relative paths in the project
- **Instructions** on how to handle automation requests
- **Best practices** for notebook naming, structure, and the Workbooks state API

### Intelligent Automation Workflow
When you ask Claude to automate a task (e.g., "I need to automate quickbooks pulls"):

1. **Checks existing notebooks** - Reviews the list of notebooks to find relevant ones
2. **Reads relevant files** - If found, uses Read tool to examine notebook cells
3. **Analyzes implementation** - Understands what the existing automation does
4. **Suggests actions** - Either modify existing notebook or create new one
5. **Creates notebooks** - If needed, creates properly structured `.ipynb` files with descriptive names
6. **Follows best practices** - Includes markdown cells for documentation, uses state API appropriately

### Benefits
- **No need to explain project structure** - Claude already knows what notebooks exist
- **Proactive exploration** - Automatically reads relevant files without being asked
- **Context-aware suggestions** - Knows whether to create or modify notebooks
- **Consistent naming** - Follows conventions like `quickbooks_sync.ipynb`
- **Best practices by default** - Applies Workbooks patterns automatically

## Design Decisions

### Why Python SDK instead of TypeScript?
- **Existing infrastructure**: We already have a Python FastAPI `engine_server.py` running
- **Unified environment**: Keeps all AI/ML dependencies in one place
- **Easy integration**: Can reuse existing HTTP communication patterns
- **No additional processes**: Doesn't require spawning a separate Node.js runtime

### Why main panel instead of sidebar?
- **AI-first experience**: Claude Code chat sessions are the primary way to interact with projects
- **Always accessible**: AI is always visible, not hidden behind a toggle
- **Focused context**: Opening files adds them as context to the AI automatically
- **Natural workflow**: Chat with Claude about your code while viewing it side-by-side

### Why SQLite for chat storage?
- **Lightweight**: No external database required
- **Portable**: User data stays local in `~/.workbooks/`
- **Fast**: Efficient querying for recent sessions
- **Familiar**: Already using SQLite for other features (secrets, state)

## User Flow

1. **Open a project** - AI chat panel is immediately visible as the main interface
2. **Fresh chat session auto-starts** - The app automatically creates a new blank chat session for this project
3. **Previous conversations available** - Past chats are preserved in the history sidebar (click the history icon to access)
4. **Start chatting** - AI is ready to help with your project
5. **Ask Claude to create or edit files** - "Create a new workbook for data processing"
6. **Files auto-open** - When Claude creates or edits a file, it automatically opens in the right panel
7. **Watch changes in real-time** - See the file content appear and update as Claude writes it
8. **File becomes focused** - The opened file automatically becomes context for Claude
9. **Continue the conversation** - Ask Claude to explain or modify the file further
10. **See progress indicators** - Track what Claude is doing (reading files, editing, running commands)
11. **Changes stream in** - See Claude's responses and code changes in real-time
12. **Sessions persist** - Chat history saved automatically and tied to this project

## Technical Implementation

### Tauri Commands (Rust)
```rust
// Session management (SQLite for UI chat history)
create_chat_session(title: String, model: Option<String>, project_root: Option<String>) -> ChatSession
list_chat_sessions() -> Vec<ChatSession>
get_chat_session(session_id: String) -> ChatSession
delete_chat_session(session_id: String)
add_message_to_session(session_id, role, content)
get_or_create_project_chat_session(project_root: String, project_name: String) -> ChatSession
  // Always creates a new chat session for the project
  // Previous sessions remain in database and accessible via history
  // Returns fresh session with empty message history

// Project context
get_project_context(project_root: String) -> ProjectContext
  // Returns: { project_name, project_root, notebooks: [{ name, path, relative_path }] }

// Claude CLI integration
check_claude_cli_installed() -> ClaudeInfo
claude_cli_get_session_name(project_root) -> String
claude_cli_stream(prompt, project_root, session_name, allowed_tools, model) -> Response
  // allowed_tools pre-approved: ["Read", "Write", "Edit", "Bash", "Glob", "Grep", "Task"]
  // Streams with real-time progress events (tool_use, content, thinking)
```

### Event System
Tauri events emitted during Claude execution:
- `claude-cli-event` with type: `content` - Streaming response text
- `claude-cli-event` with type: `tool_use` - Claude is using a tool (Read, Edit, etc.)
- `claude-cli-event` with type: `tool_result` - Tool execution result
- `claude-cli-event` with type: `thinking` - Claude is planning

#### Auto-File-Opening
When Claude uses the `Write` or `Edit` tools, the frontend automatically:
1. Detects the tool event and extracts the `file_path` from the tool input
2. Determines the file type based on extension (.ipynb → workbook, .py → python, etc.)
3. Calls `onOpenFile()` to automatically open the file in the right panel
4. The file viewer immediately shows the content as Claude writes it
5. This creates a seamless experience where you can watch Claude create and modify files in real-time

### Focused File Context
When a file is open, its path is automatically prepended to user prompts:
```
[Focused file: /path/to/file.py]

User's actual question here
```

This gives Claude automatic context about what file the user is working with.

### Model Selection

Users can choose which Claude model to use for their conversations:
- **Haiku** - Fastest, most cost-effective for simple tasks
- **Sonnet** - Balanced performance and capability (default)
- **Opus** - Most capable for complex reasoning and code generation

The model selector appears in the chat header next to the "Claude Code" title. Model preference is saved in localStorage and persists across sessions.

## Security & Permissions

- **No API key needed**: Uses Claude Code CLI authentication (user's existing Claude account)
- **Project-scoped access**: Claude only has access to files within the current project directory
- **Pre-approved tools**: Common operations (Read, Write, Edit, Bash, etc.) are automatically allowed
  - The app context itself provides the permission boundary
  - Users chose to use Workbooks for automation - permission is implicit
  - Progress indicators provide transparency about what Claude is doing
- **Local chat history**: SQLite database stores chat sessions locally at `~/.workbooks/chat_sessions.db`
- **Session persistence**: Claude Code CLI manages session state for conversation continuity

## Future Enhancements

- **Inline code suggestions**: Apply agent's code changes directly to workbooks
- **Voice input**: Speech-to-text for faster interaction
- **Session export**: Download chat sessions as markdown or PDF
- **Multi-project context**: Switch between projects without losing chat history
- **Agent templates**: Pre-configured agents for common tasks (debugging, refactoring, etc.)
- **Markdown rendering**: Rich formatting for code blocks, lists, and other markdown elements in responses
