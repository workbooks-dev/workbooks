# MCP Server Integration

## Overview

The MCP (Model Context Protocol) server enables Claude Desktop to interact with Workbooks projects directly. Users can ask Claude to create notebooks, run pipelines, inspect outputs, and manage project workflows through natural language.

## Key Capabilities

1. **Notebook Creation** - Claude can scaffold new notebooks based on user requirements
2. **Notebook Execution** - Run notebooks and stream/return outputs
3. **Project Inspection** - List notebooks, read outputs, view run history
4. **Package Management** - Check installed packages, suggest installations
5. **Scheduling** - Schedule notebook runs (future feature)
6. **Notifications** - Claude receives notifications about runs and events

## Architecture

### Integration with Engine Server

The MCP server is **integrated into the existing FastAPI engine server** (port 8765) using **FastMCP**. This provides:
- Single process for both Tauri app and Claude Desktop communication
- Shared engine/kernel management
- Consistent execution behavior
- Simplified deployment

```
┌─────────────────────────────────────────────────────────────┐
│                    Tauri App (React)                        │
└────────────────┬────────────────────────────────────────────┘
                 │ HTTP (localhost:8765)
                 ▼
┌─────────────────────────────────────────────────────────────┐
│            FastAPI Engine Server (port 8765)                │
│  ┌──────────────────┐  ┌──────────────────┐                │
│  │  HTTP Endpoints  │  │  FastMCP Server  │                │
│  │  (Tauri calls)   │  │  (Claude calls)  │                │
│  └──────────────────┘  └──────────────────┘                │
│                                                              │
│         Shared Jupyter AsyncKernelManager                   │
└─────────────────────────────────────────────────────────────┘
                 │
                 ▼
         ┌──────────────────────┐
         │  Claude Desktop App  │
         │  (MCP Client)        │
         └──────────────────────┘
```

### Per-Project MCP Configuration

Each Workbooks project gets its own MCP server instance configured in Claude Desktop:

```json
{
  "mcpServers": {
    "workbooks-my-pipeline": {
      "command": "workbooks",
      "args": ["mcp", "--project", "/Users/you/Projects/my-pipeline"]
    },
    "workbooks-data-analysis": {
      "command": "workbooks",
      "args": ["mcp", "--project", "/Users/you/Projects/data-analysis"]
    }
  }
}
```

**Benefits:**
- Each project is a separate MCP connection
- No confusion about which project Claude is working with
- Can have multiple projects configured side-by-side
- MCP server knows project context for all operations

### One-Click Claude Integration (Workbooks App)

**Instead of manual JSON editing**, the Workbooks app provides:

#### "Add to Claude Desktop" Button
- Located in Project Settings or Sidebar
- Automatically updates Claude Desktop config file:
  - macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
  - Windows: `%APPDATA%\Claude\claude_desktop_config.json`
  - Linux: `~/.config/Claude/claude_desktop_config.json`
- Adds entry: `workbooks-{project-name}` with correct project path
- Shows success/error messages
- Warns if `workbooks` CLI not in PATH

#### "Manage Claude Projects" Dialog
- Lists all `workbooks-*` entries in Claude config
- Checkboxes to enable/disable projects
- "Remove all other Workbooks projects" button
- Safe config file updates with backup
- Validation before writing

**Implementation:**
- Rust handles JSON read/write via `serde_json`
- Create backup before modifying config
- Validate `workbooks` CLI is installed/accessible
- Show helpful error messages if config is malformed

## MCP Tools

### 1. `create_notebook`
Create a new notebook in the project.

**Parameters:**
- `name` - Notebook filename (e.g., "data_cleaning.ipynb")
- `description` - What the notebook should do
- `cells` - Optional array of initial cells (code/markdown)
- `template` - Optional template name ("blank", "analysis", "ml_training")

**Behavior:**
- Creates `.ipynb` file in project root or `notebooks/` directory
- Scaffolds with common imports if requested
- Returns info about installed packages (from `pyproject.toml`)
- Suggests packages to install via `!uv add <package>`

**Returns:**
- File path
- List of pre-installed packages
- Success status

### 2. `run_notebook`
Execute a notebook using the Workbooks engine.

**Parameters:**
- `notebook_path` - Relative path to notebook
- `stream_output` - Boolean (default: false)
- `return_format` - "cells" | "markdown" | "both" (default: "cells")

**Behavior:**
- Uses the **full engine/checkpoint system** (same as Tauri app)
- Executes via AsyncKernelManager
- Creates run history with source: "Claude Desktop: {user_request}"
- Supports streaming output if requested
- Returns final cell outputs or markdown rendering

**Returns:**
- Run ID
- Cell outputs (if `return_format` includes "cells")
- Markdown rendering (if `return_format` includes "markdown")
- Execution status
- Error details if failed

### 3. `list_notebooks`
List all notebooks in the project.

**Parameters:**
- `include_outputs` - Boolean (default: false)

**Returns:**
- Array of notebook paths
- Last modified timestamps
- Last run info (if available)
- Cell count
- Output summaries (if `include_outputs=true`)

### 4. `read_notebook`
Read notebook contents and outputs.

**Parameters:**
- `notebook_path` - Relative path to notebook
- `include_outputs` - Boolean (default: true)

**Returns:**
- Cell contents (code + markdown)
- Cell outputs (if `include_outputs=true`)
- Metadata (kernel, language version)

### 5. `modify_notebook`
Update existing notebook cells.

**Parameters:**
- `notebook_path` - Relative path to notebook
- `operations` - Array of `{type: "insert" | "replace" | "delete", index: number, content?: string}`

**Returns:**
- Updated notebook info
- Success status

### 6. `list_packages`
List installed packages in project environment.

**Parameters:** None

**Returns:**
- Installed packages from `pyproject.toml` and `uv.lock`
- Package versions
- Dependencies

### 7. `get_project_info`
Get project metadata.

**Parameters:** None

**Returns:**
- Project name
- Project root path
- Python version
- Installed packages
- Existing notebooks
- Recent runs

### 8. `get_secrets` (Optional, User-Controlled)
Read secret keys (read-only).

**Parameters:** None

**Returns:**
- Secret keys (names only, or values if user enables)
- User can **disable this feature** in Workbooks app settings

**Security:**
- Default: keys only, no values
- User opt-in required for value access
- Settings toggle: "Allow Claude Desktop to read secret values"

### 9. `inspect_runs`
View run history.

**Parameters:**
- `notebook_path` - Optional, filter by notebook
- `limit` - Max runs to return (default: 10)

**Returns:**
- Run ID
- Notebook name
- Start/end time
- Status (success/failed)
- Source ("Tauri App", "Claude Desktop: {request}", "Scheduled")
- Error summary if failed

### 10. `schedule_notebook` (Future)
Schedule a notebook to run on a cron schedule.

**Parameters:**
- `notebook_path` - Relative path to notebook
- `cron_expression` - Cron schedule
- `enabled` - Boolean (default: true)

**Returns:**
- Schedule ID
- Next run time

### 11. `notify_user` (Future)
Send notification to Claude Desktop about events.

**Parameters:**
- `message` - Notification content
- `priority` - "low" | "normal" | "high"

**Behavior:**
- Adds to pending notifications queue
- Claude can check notifications via `get_notifications` tool
- Use cases: "Notebook finished running", "Cell executed in 30s", "Error in scheduled run"

## Run History & Source Tracking

All notebook executions via MCP create run history entries with:
- **Source field**: `"Claude Desktop: {original_user_request}"`
- Example: `"Claude Desktop: Run the data cleaning pipeline"`
- Helps users understand which runs were automated vs manual
- Visible in Workbooks app run history UI

## User Privacy & Security Controls

Users can disable MCP features in Workbooks app settings:
- ☑ Allow Claude to list project notebooks
- ☑ Allow Claude to read notebook outputs
- ☑ Allow Claude to create/modify notebooks
- ☑ Allow Claude to run notebooks
- ☑ Allow Claude to read secret keys (names only)
- ☐ Allow Claude to read secret values (disabled by default)
- ☑ Allow Claude to read project metadata

These settings are stored in `.workbooks/config.toml` and enforced by the MCP server.

## Notification System (Future)

**Goal:** Allow Claude to be notified of events asynchronously.

**Design options:**
1. **Polling** - Claude periodically calls `get_notifications` tool
2. **SSE/WebSocket** - MCP server pushes notifications (if MCP supports)
3. **Prompt resources** - Claude Desktop resources that update (MCP feature)

**Use cases:**
- Scheduled notebook completed
- Long-running notebook finished
- Error in execution
- Custom user notifications via `workbooks.notify("message")`

**Implementation:** TBD based on MCP protocol capabilities.

## CLI Command

```bash
# Start MCP server for a project (called by Claude Desktop)
workbooks mcp --project /path/to/project

# Ensure engine server is running
# Start FastMCP server on stdio (MCP protocol)
# Register all MCP tools
```

**Requirements:**
- `workbooks` CLI must be in user's PATH
- Or bundled with Tauri app and symlinked to system PATH during installation

## Error Handling

MCP tools should return clear error messages:
- "Project not found at path: /path/to/project"
- "Notebook not found: notebooks/missing.ipynb"
- "Execution failed: ModuleNotFoundError: No module named 'pandas'. Try: !uv add pandas"
- "Feature disabled: User has disabled notebook execution in Workbooks settings"

## Future Enhancements

- **Visual pipeline editing** - Claude can modify React Flow DAG
- **State system integration** - Once state system is built, add `get_state` and `set_state` tools
- **Smart scheduling** - Claude suggests optimal run times based on dependencies
- **Collaborative annotations** - Claude can add comments/markdown cells explaining code
- **Diff/review** - Claude can review notebook changes before running
