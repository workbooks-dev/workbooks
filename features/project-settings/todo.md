# Project Settings - To Do

## Global Configuration (Shared with CLI)

### Backend - Global Config Management
- [ ] Create global config system in `src/config.rs`
  - [ ] `GlobalConfig` struct matching TOML structure
  - [ ] `load_global_config()` - Read from `~/.tether/config.toml`
  - [ ] `save_global_config()` - Write to `~/.tether/config.toml`
  - [ ] Create `~/.tether/` directory if doesn't exist
  - [ ] Handle malformed config gracefully
- [ ] Implement Tauri commands for global config
  - [ ] `get_global_config()` - Return entire config
  - [ ] `set_default_project(path)` - Set default project
  - [ ] `get_default_project()` - Get current default
  - [ ] `unset_default_project()` - Remove default
  - [ ] `add_recent_project(path)` - Add to recent list
  - [ ] `get_recent_projects()` - Get up to 10 recent
- [ ] Update recent projects on project open/switch
  - [ ] Auto-add to recent list
  - [ ] Maintain chronological order
  - [ ] Limit to 10 entries

### Frontend - Default Project UI
- [ ] Add "Default Project" section to Project Info
  - [ ] "Set as Default Project" button
  - [ ] Shows "✓ Default Project" badge if current
  - [ ] Confirm before changing default
  - [ ] Show success notification
- [ ] Update app launch behavior
  - [ ] Check for default project on launch
  - [ ] Auto-open default if set
  - [ ] Show welcome screen if no default
- [ ] Update Welcome screen
  - [ ] Highlight default project in recent list
  - [ ] Show badge/indicator for default
  - [ ] Quick action to change default

## Settings Tab Component

- [ ] Create ProjectSettings.jsx component
  - [ ] Open as tab (not modal)
  - [ ] Multi-section layout
  - [ ] Save/cancel functionality (if needed)

- [ ] Project Info section
  - [ ] Editable project name field
  - [ ] Read-only project path with copy button
  - [ ] Created date display
  - [ ] Validation for project name

- [ ] Python Environment section
  - [ ] Display venv path
  - [ ] Show Python version
  - [ ] Show venv size (optional)
  - [ ] Installed packages table
  - [ ] Search/filter packages
  - [ ] Package version display

- [ ] Package Management
  - [ ] "+ Add Package" button
  - [ ] Package install dialog
  - [ ] Package name input with validation
  - [ ] Version specification (optional)
  - [ ] Installation progress indicator
  - [ ] Success/error notifications
  - [ ] Remove package button (X per package)
  - [ ] Remove confirmation dialog

- [ ] Export & Sharing section
  - [ ] "Export as ZIP" button
  - [ ] Export dialog (choose location)
  - [ ] Export progress indicator
  - [ ] Success notification with path
  - [ ] Explanation of what's included/excluded

- [ ] CLI Integration section
  - [ ] CLI installation status display
  - [ ] "Install CLI" button (if not installed)
  - [ ] Installation progress/success notification
  - [ ] Show CLI installation path
  - [ ] Verify CLI is accessible from terminal
  - [ ] Link to CLI documentation

## Claude Desktop Integration

### Backend - Claude Config Management
- [ ] Implement Claude Desktop config detection
  - [ ] `get_claude_config_path()` - Locate config file
    - [ ] macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
    - [ ] Windows: `%APPDATA%\Claude\claude_desktop_config.json`
    - [ ] Linux: `~/.config/Claude/claude_desktop_config.json`
  - [ ] Handle missing config file (create if needed)
  - [ ] Validate JSON structure before modifying
- [ ] Implement Claude config modification commands
  - [ ] `add_to_claude_desktop(project_path, project_name)` - Add MCP entry
  - [ ] `list_claude_tether_projects()` - List all `tether-*` entries
  - [ ] `remove_from_claude_desktop(project_name)` - Remove entry
  - [ ] Backup config before modifying
  - [ ] Validate changes after writing
- [ ] Implement CLI installation commands
  - [ ] `get_cli_installation_status()` - Check if `tether` in PATH
  - [ ] `install_cli_to_path()` - Copy binary to system location
    - [ ] macOS/Linux: `/usr/local/bin/tether`
    - [ ] Windows: `%LOCALAPPDATA%\Programs\Tether\bin\`
  - [ ] Set executable permissions (Unix)
  - [ ] Verify installation succeeded

### Frontend - Claude Desktop UI
- [ ] Add "Claude Desktop" section to settings
  - [ ] "Add to Claude Desktop" button
    - [ ] Check if `tether` CLI is installed first
    - [ ] Warn if CLI not installed, offer to install
    - [ ] Call `add_to_claude_desktop` command
    - [ ] Show success message with restart instructions
    - [ ] Display MCP server entry name
  - [ ] "Manage Claude Projects" button/dialog
    - [ ] Opens modal/dialog with project list
    - [ ] Shows all Tether MCP entries
    - [ ] Checkboxes to enable/disable
    - [ ] "Remove all other Tether projects" button
    - [ ] Confirm before removing projects
    - [ ] Update config on save
  - [ ] Show current Claude integration status
    - [ ] "✓ Added to Claude Desktop" if configured
    - [ ] Show MCP server name
    - [ ] "Not configured" if missing
- [ ] Add CLI installation UI
  - [ ] Show installation status (installed/not installed)
  - [ ] "Install CLI" button with progress indicator
  - [ ] Success notification with path
  - [ ] Link to test CLI with example command

## Backend (Rust)

- [ ] Project metadata management
  - [ ] Store project info in `.tether/project.json`
  - [ ] `get_project_info()` command
  - [ ] `update_project_name()` command
  - [ ] Update `.tether` shortcut file on name change

- [ ] Package management commands
  - [ ] `list_packages()` - Get installed packages from venv
  - [ ] `install_package(name, version)` - Install via uv
  - [ ] `uninstall_package(name)` - Uninstall via uv
  - [ ] `get_package_info(name)` - Version, size, etc.

- [ ] Virtual environment info
  - [ ] `get_venv_info()` - Path, Python version, size
  - [ ] Calculate venv size
  - [ ] Get Python version from venv

- [ ] Export functionality
  - [ ] `export_project(dest_path)` - Create ZIP
  - [ ] Include: workbooks, files, pyproject.toml, secrets
  - [ ] Exclude: .tether/, venv, cache
  - [ ] Progress callback for large projects
  - [ ] Handle encrypted secrets (`.env.tether`)

## Integration

- [ ] Wire up gear icon in sidebar
  - [ ] Click → Open settings tab
  - [ ] Tooltip

- [ ] Tab system integration
  - [ ] Add `settings` tab type
  - [ ] Settings tab icon
  - [ ] Handle unsaved changes (if applicable)

- [ ] Update project name across app
  - [ ] Window title
  - [ ] `.tether` shortcut file
  - [ ] Project metadata

## Default Packages

- [ ] Implement default package system
  - [ ] Read from project-defaults.md or config
  - [ ] Install on project creation
  - [ ] Allow customization (future)

## App Installation

### Tauri Installer Configuration
- [ ] Add post-install script for CLI installation
  - [ ] Prompt user: "Install tether CLI to system PATH?"
  - [ ] Checkbox: "☑ Install CLI" (checked by default, opt-out)
  - [ ] Run `install_cli_to_path()` if accepted
  - [ ] Show installation success/failure
  - [ ] Test CLI accessibility before finishing
- [ ] Bundle CLI binary with app installer
  - [ ] Include `tether` binary in app bundle
  - [ ] Platform-specific bundling (macOS .app, Windows installer, Linux AppImage)
- [ ] Test installation on all platforms
  - [ ] macOS installer with CLI option
  - [ ] Windows installer with CLI option
  - [ ] Linux installer with CLI option

## Import/Import

- [ ] Import exported project
  - [ ] Detect `.tether` shortcut in ZIP
  - [ ] Extract to chosen location
  - [ ] Initialize as new project
  - [ ] Recreate venv
  - [ ] Install packages from pyproject.toml
  - [ ] Prompt for secrets

## Advanced Settings (Future)

- [ ] Autosave configuration
  - [ ] Interval slider
  - [ ] Global enable/disable

- [ ] Execution settings
  - [ ] Kernel timeout
  - [ ] Max output size
  - [ ] Clear outputs on save toggle

- [ ] Storage settings
  - [ ] Run history retention
  - [ ] Cache size limits
  - [ ] Cleanup options

## UX Improvements

- [ ] Keyboard shortcut to open settings (Cmd/Ctrl+,)
- [ ] Search within settings
- [ ] Help tooltips for each setting
- [ ] Restore defaults button
- [ ] Settings validation
