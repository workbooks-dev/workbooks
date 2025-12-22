# AI Assistant

## Overview

The AI Assistant provides inline chat capabilities powered by the Claude Agent SDK. It allows users to get help with their workbooks, debug code, automate tasks, and interact with their project using natural language.

## Architecture

### Backend (Python)
- **Claude Agent SDK** installed in `~/.tether/engine/.venv`
- **Agent endpoint** in `engine_server.py` at `/agent/chat`
- **Streaming responses** via Server-Sent Events (SSE)
- **API key** retrieved securely from system keychain

### Frontend (React)
- **AiSidebar.jsx** - Main chat interface component
- **Toggleable sidebar** on the right side of the screen
- **Floating action button** to open/close when enabled
- **Chat sessions** persist across app restarts

### Data Storage (SQLite)
- **Chat sessions database** at `~/.tether/chat_sessions.db`
- **Tables:**
  - `sessions` - Session metadata (id, title, created_at, updated_at)
  - `messages` - Chat messages (session_id, role, content, timestamp)

## Design Decisions

### Why Python SDK instead of TypeScript?
- **Existing infrastructure**: We already have a Python FastAPI `engine_server.py` running
- **Unified environment**: Keeps all AI/ML dependencies in one place
- **Easy integration**: Can reuse existing HTTP communication patterns
- **No additional processes**: Doesn't require spawning a separate Node.js runtime

### Why right sidebar instead of left?
- **File browser preservation**: The left sidebar is for file navigation (core feature)
- **Optional feature**: AI assistant is an opt-in enhancement, not core navigation
- **Visual separation**: Clearly distinguishes between project files and AI chat
- **Toggleable**: Can be hidden when not needed without affecting file access

### Why SQLite for chat storage?
- **Lightweight**: No external database required
- **Portable**: User data stays local in `~/.tether/`
- **Fast**: Efficient querying for recent sessions
- **Familiar**: Already using SQLite for other features (secrets, state)

## User Flow

1. **Enable AI Features** in Settings (requires Anthropic API key)
2. **Floating button** appears in bottom-right when AI enabled
3. **Click button** to open AI sidebar
4. **Create new chat** or select from recent sessions
5. **Type message** and press Enter to send
6. **Response streams** from Claude Agent SDK in real-time
7. **Sessions persist** automatically for future reference

## Technical Implementation

### Tauri Commands (Rust)
```rust
create_chat_session(title: String) -> ChatSession
list_chat_sessions() -> Vec<ChatSession>
get_chat_session(session_id: String) -> ChatSession
delete_chat_session(session_id: String)
add_message_to_session(session_id, role, content)
send_agent_message(session_id, message, project_root) -> String
```

### Engine Server Endpoint (Python)
```python
POST /agent/chat
{
  "session_id": "uuid",
  "message": "user prompt",
  "api_key": "sk-ant-...",
  "project_root": "/path/to/project"
}
```

Streams SSE events:
- `{type: "start"}` - Request started
- `{type: "chunk", content: "..."}` - Partial response
- `{type: "complete", full_response: "..."}` - Done
- `{type: "error", message: "..."}` - Error occurred

### Agent SDK Configuration
```python
options = ClaudeAgentOptions(
    allowed_tools=["Read", "Bash", "Glob", "Grep", "Edit", "Write"]
)
```

The agent can read files, run commands, search code, and edit workbooks within the project context.

## Security

- **API key** stored in system keychain (macOS Keychain, Windows Credential Manager, Linux keyring)
- **Never logged**: API keys are never written to logs or console
- **Local-only**: Chat sessions stored locally, never sent to external servers except Anthropic API
- **Project context**: Agent only has access to files within the current project directory

## Future Enhancements

- **Tool use visibility**: Show when agent is reading files or running commands
- **Inline code suggestions**: Apply agent's code changes directly to workbooks
- **Voice input**: Speech-to-text for faster interaction
- **Session export**: Download chat sessions as markdown or PDF
- **Multi-project context**: Switch between projects without losing chat history
- **Agent templates**: Pre-configured agents for common tasks (debugging, refactoring, etc.)
