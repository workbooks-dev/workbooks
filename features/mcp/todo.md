# MCP Server - Todo

## Phase 1: Core Infrastructure

### CLI & Engine Server Integration
- [ ] Add `workbooks mcp` CLI subcommand
  - [ ] Accept `--project <path>` argument
  - [ ] Validate project path exists and has `.workbooks/` directory
  - [ ] Start or connect to existing engine server for project
- [ ] Integrate FastMCP into `engine_server.py`
  - [ ] Add FastMCP dependency to engine requirements
  - [ ] Create MCP server instance alongside FastAPI
  - [ ] Run MCP on stdio (for Claude Desktop communication)
  - [ ] Share AsyncKernelManager between HTTP and MCP handlers
- [ ] Add MCP feature flag/settings to `.workbooks/config.toml`
  - [ ] `mcp.enabled = true`
  - [ ] `mcp.allow_list_notebooks = true`
  - [ ] `mcp.allow_read_outputs = true`
  - [ ] `mcp.allow_create_notebooks = true`
  - [ ] `mcp.allow_modify_notebooks = true`
  - [ ] `mcp.allow_run_notebooks = true`
  - [ ] `mcp.allow_read_secret_keys = true`
  - [ ] `mcp.allow_read_secret_values = false` (default disabled)
  - [ ] `mcp.allow_read_metadata = true`

## Phase 2: Core MCP Tools

### Notebook Listing & Reading
- [ ] Implement `list_notebooks` tool
  - [ ] Scan project for `.ipynb` files
  - [ ] Return paths, timestamps, cell counts
  - [ ] Optional: include output summaries
  - [ ] Check `mcp.allow_list_notebooks` setting
- [ ] Implement `read_notebook` tool
  - [ ] Load notebook JSON
  - [ ] Return cell contents and outputs
  - [ ] Include metadata (kernel, language version)
  - [ ] Check `mcp.allow_read_outputs` setting

### Notebook Creation
- [ ] Implement `create_notebook` tool
  - [ ] Generate `.ipynb` JSON structure
  - [ ] Support templates ("blank", "analysis", "ml_training")
  - [ ] Scaffold with common imports if requested
  - [ ] Save to project root or `notebooks/` directory
  - [ ] Return installed packages from `pyproject.toml`
  - [ ] Check `mcp.allow_create_notebooks` setting

### Notebook Execution
- [ ] Implement `run_notebook` tool
  - [ ] Reuse existing engine execution logic
  - [ ] Create run history with source: "Claude Desktop: {request}"
  - [ ] Support `stream_output` parameter
  - [ ] Support `return_format`: "cells", "markdown", "both"
  - [ ] Return run ID, outputs, status
  - [ ] Handle execution errors gracefully
  - [ ] Check `mcp.allow_run_notebooks` setting

### Notebook Modification
- [ ] Implement `modify_notebook` tool
  - [ ] Support operations: insert, replace, delete cells
  - [ ] Validate cell indices
  - [ ] Update notebook JSON
  - [ ] Return success status
  - [ ] Check `mcp.allow_modify_notebooks` setting

## Phase 3: Project Information

### Package Management
- [ ] Implement `list_packages` tool
  - [ ] Read `pyproject.toml` dependencies
  - [ ] Parse `uv.lock` for versions
  - [ ] Return package names and versions

### Project Metadata
- [ ] Implement `get_project_info` tool
  - [ ] Return project name, root path
  - [ ] Return Python version
  - [ ] Return installed packages
  - [ ] Return list of notebooks
  - [ ] Return recent runs
  - [ ] Check `mcp.allow_read_metadata` setting

### Secrets Access
- [ ] Implement `get_secrets` tool
  - [ ] Read from secrets store/keychain
  - [ ] Default: return keys only (no values)
  - [ ] Respect `mcp.allow_read_secret_keys` setting
  - [ ] Respect `mcp.allow_read_secret_values` setting
  - [ ] Return clear error if disabled

### Run History
- [ ] Implement `inspect_runs` tool
  - [ ] Query run history from `.workbooks/runs/`
  - [ ] Support filtering by notebook
  - [ ] Support limit parameter
  - [ ] Return run ID, notebook, time, status, source
  - [ ] Include error summaries for failed runs

## Phase 4: One-Click Claude Integration (Tauri App)

### Claude Config Management
- [ ] Add "Claude Desktop" settings section to Project Settings
- [ ] Implement "Add to Claude Desktop" button
  - [ ] Detect OS and locate Claude config file
    - [ ] macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
    - [ ] Windows: `%APPDATA%\Claude\claude_desktop_config.json`
    - [ ] Linux: `~/.config/Claude/claude_desktop_config.json`
  - [ ] Read existing config (create if doesn't exist)
  - [ ] Add/update entry: `workbooks-{project-name}`
  - [ ] Include correct `command` and `args` with project path
  - [ ] Validate `workbooks` CLI is in PATH (warn if not)
  - [ ] Backup config before modifying
  - [ ] Write updated config
  - [ ] Show success message: "Added to Claude Desktop. Restart Claude to connect."
  - [ ] Show error if config malformed or write fails

### Multi-Project Management
- [ ] Implement "Manage Claude Projects" dialog/section
  - [ ] Read Claude config file
  - [ ] Parse and list all `workbooks-*` entries
  - [ ] Show checkboxes to enable/disable each project
  - [ ] Add "Remove all other Workbooks projects" button
  - [ ] Confirm before removing projects
  - [ ] Update config file safely
  - [ ] Show current project status (active/inactive)

### CLI Installation Helper
- [ ] Detect if `workbooks` CLI is accessible
- [ ] Show installation instructions if missing
- [ ] Consider: symlink bundled Tauri app CLI to system PATH on install
- [ ] Add Tauri installer script to add CLI to PATH

## Phase 5: Error Handling & UX

### Error Messages
- [ ] Standardize error responses for all MCP tools
- [ ] Clear messages for common issues:
  - [ ] Project not found
  - [ ] Notebook not found
  - [ ] Execution failed (with helpful suggestions)
  - [ ] Feature disabled by user
  - [ ] Invalid parameters
- [ ] Include suggested fixes in error messages

### Settings UI
- [ ] Add MCP settings panel to Workbooks app
- [ ] Toggles for each MCP permission
- [ ] Explain what each permission allows
- [ ] Show which features Claude can access
- [ ] Warning for enabling secret value access

## Phase 6: Advanced Features (Future)

### Scheduling
- [ ] Implement `schedule_notebook` tool
  - [ ] Create cron schedule for notebook
  - [ ] Store in `.workbooks/schedules/`
  - [ ] Return schedule ID and next run time
  - [ ] Integrate with scheduling system (once built)

### Notifications
- [ ] Design notification system architecture
  - [ ] Evaluate MCP protocol capabilities (polling vs push)
  - [ ] Choose implementation: polling, SSE, or MCP resources
- [ ] Implement notification queue
  - [ ] Store pending notifications
  - [ ] Add `get_notifications` tool for Claude to check
- [ ] Add notification sources:
  - [ ] Scheduled runs completing
  - [ ] Long-running executions finishing
  - [ ] Errors in background runs
  - [ ] Custom `workbooks.notify("message")` from Python
- [ ] UI for notification preferences

### Documentation
- [ ] Write MCP server usage guide
- [ ] Create example Claude Desktop conversations
- [ ] Document Claude config setup (manual and one-click)
- [ ] Add troubleshooting section
- [ ] Video/GIF walkthrough of "Add to Claude Desktop" feature

## Testing
- [ ] Unit tests for each MCP tool
- [ ] Integration tests with actual Claude Desktop
- [ ] Test config file read/write edge cases
- [ ] Test permission enforcement
- [ ] Test error handling and messages
- [ ] Test concurrent execution (Tauri + Claude both running notebooks)
