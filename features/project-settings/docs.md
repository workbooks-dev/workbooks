# Project Settings

## Overview

Project settings provide configuration and management for the entire Workbooks project. Accessed via the gear icon at the bottom of the sidebar.

**See `/project-defaults.md` for default packages and project initialization.**

## Global Configuration

Workbooks maintains a **global config file** at `~/.workbooks/config.toml` that stores settings shared between the desktop app and CLI:
- Default project
- Recent projects list
- App preferences (theme, etc.)
- CLI preferences

Both the app and CLI read/write this config, enabling seamless integration.

## Design Philosophy

**One Place for Project Config:**
- Edit project name
- Manage Python packages
- Configure project settings
- Export/share project

**Tab-Based UI:**
- Opens in new tab (not modal)
- Consistent with rest of app
- Can keep open while working

## User Experience

### Access

**Sidebar Gear Icon:**
- Located at bottom of sidebar
- Click → Opens settings tab
- Keyboard shortcut (future): Cmd/Ctrl+,

### Settings Tab

**Multiple Sections:**

#### 1. Project Info

**Project Name:**
- Editable text field
- Updates `.workbooks` shortcut file
- Updates window title
- Validation (no special chars)

**Project Path:**
- Read-only display
- Absolute path to project root
- Copy button for convenience

**Created Date:**
- When project was initialized
- Read-only

**Default Project:**
- "Set as Default Project" button
- Marks this project as the default for CLI and app
- Updates global config (`~/.workbooks/config.toml`)
- Shows "✓ Default Project" badge if currently default
- On app launch, default project auto-opens
- CLI uses default if no `--project` flag or cwd detection

#### 2. Python Environment

**Virtual Environment:**
- Shows venv path (centralized: `~/.workbooks/venvs/{project-name}-{hash}`)
- Python version
- Venv size

**Installed Packages:**
- Table of currently installed packages
- Columns: Name, Version, Size
- Search/filter packages
- Sync status indicator

**Package Management:**
- "+ Add Package" button
- Opens package install dialog
- Suggests popular packages
- Shows installation progress

**Remove Package:**
- Click X next to package
- Confirmation dialog
- Updates `pyproject.toml` and syncs

**Default Packages:**
- List of packages installed in new projects
- See `project-defaults.md` for details
- Editable (future)

#### 3. Export & Sharing

**Export Project:**
- "Export as ZIP" button
- Creates archive of entire project
- Includes:
  - All workbooks
  - All files
  - `pyproject.toml` (dependencies)
  - Encrypted secrets (`.env.workbooks`)
  - Project structure
- Excludes:
  - `.workbooks/` directory (state, runs, etc.)
  - Virtual environment
  - Python cache files

**Recipient Setup:**
- Open exported ZIP in Workbooks
- Workbooks recreates venv and installs packages
- Prompts to add their own secrets
- Ready to run

**Use Cases:**
- Share analysis with colleagues
- Backup project
- Move between machines
- Collaboration

#### 4. Advanced Settings (Future)

**Autosave:**
- Interval (seconds)
- Enable/disable globally

**Execution:**
- Default kernel timeout
- Max output size
- Clear outputs on save

**Storage:**
- Run history retention (default 30)
- Cache size limits
- Cleanup old checkpoints

## Package Management

### Add Package Flow

1. Click "+ Add Package"
2. Package install dialog opens:
   - Search PyPI (future: autocomplete)
   - Or enter package name directly
   - Optional: specify version
3. Click "Install"
4. Shows progress:
   - "Installing [package]..."
   - Progress bar (if available)
5. Updates `pyproject.toml`
6. Runs `uv sync` to install
7. Success notification
8. Package appears in list

### Remove Package Flow

1. Click X next to package in list
2. Confirmation: "Remove [package]? This will update pyproject.toml and uninstall the package."
3. Click "Remove"
4. Updates `pyproject.toml`
5. Runs `uv sync` to uninstall
6. Package removed from list

### Default Packages

**Installed in every new project:**
- See `/project-defaults.md` for full list
- Core packages: jupyter, ipykernel, nbformat
- Common data packages: pandas, numpy, matplotlib, etc.
- Workbooks integration: cloudpickle (for state)

**Customization (Future):**
- Edit default package list
- Create project templates
- Different defaults for different types of projects

#### 5. CLI Integration

**CLI Installation:**
- "Install CLI" button (if not already installed)
  - Copies `workbooks` binary to system PATH
  - Shows installation progress and success
  - Verifies accessibility
- Shows CLI installation status (installed/not installed)
- Installation path display

**Claude Desktop Integration:**
- "Add to Claude Desktop" button
  - Automatically updates Claude Desktop config
  - Adds MCP server entry for this project
  - Shows success message with restart instructions
- "Manage Claude Projects" button
  - Lists all Workbooks projects in Claude config
  - Enable/disable projects
  - Remove other Workbooks projects
  - Shows current project status

## Integration Points

### With Python Backend

**Tauri Commands:**
- `get_project_info()` - Project metadata
- `update_project_name(name)` - Rename project
- `list_packages()` - Installed packages
- `install_package(name, version)` - Add package
- `uninstall_package(name)` - Remove package
- `export_project(dest_path)` - Create ZIP
- `get_venv_info()` - Venv path, size, Python version

### With Global Config

**Global Config Commands:**
- `get_global_config()` - Read `~/.workbooks/config.toml`
- `set_default_project(path)` - Set default project
- `get_default_project()` - Get current default
- `unset_default_project()` - Remove default
- `add_recent_project(path)` - Add to recent list
- `get_recent_projects()` - Get recent projects

### With Claude Desktop

**Claude Config Commands:**
- `get_claude_config_path()` - Locate Claude config file
- `add_to_claude_desktop(project_path, project_name)` - Add MCP entry
- `list_claude_workbooks_projects()` - List all Workbooks MCP servers
- `remove_from_claude_desktop(project_name)` - Remove MCP entry
- `get_cli_installation_status()` - Check if CLI installed
- `install_cli_to_path()` - Install CLI binary

### With uv

**Package Operations:**
- All package changes go through `uv`
- Updates `pyproject.toml`
- Runs `uv sync` to apply changes
- Handles dependencies automatically

### With Sidebar

**Gear Icon:**
- Visual indicator for settings access
- Tooltip: "Project Settings"
- Always visible at bottom

### With Navigation

**Settings Tab:**
- Opens in tab system
- Can switch between settings and workbooks
- Unsaved changes warning (if applicable)

## Technical Implementation

### Project Metadata Storage

**`.workbooks/project.json`:**
```json
{
  "name": "My Project",
  "created_at": 1234567890,
  "project_root": "/Users/name/Projects/my-project",
  "python_version": "3.12",
  "venv_path": "/Users/name/.workbooks/venvs/my-project-abc123"
}
```

### Export Format

**ZIP structure:**
```
my-project.zip
├── notebooks/
│   ├── notebook1.ipynb
│   └── notebook2.ipynb
├── data/
│   └── data.csv
├── pyproject.toml
├── uv.lock
├── .env.workbooks (encrypted secrets)
└── My Project.workbooks (shortcut file)
```

**Import Flow:**
1. User extracts ZIP (or Workbooks does it)
2. Workbooks detects `.workbooks` shortcut file
3. Opens as new project
4. Creates venv at `~/.workbooks/venvs/{name}-{hash}`
5. Runs `uv sync` to install packages
6. Prompts for secrets (if `.env.workbooks` exists)
7. Ready to use

## Future Enhancements

**Project Templates:**
- Data analysis starter
- Machine learning project
- Web scraping template
- Custom templates

**Git Integration:**
- Show Git status
- Commit/push shortcuts
- .gitignore management

**Cloud Sync:**
- Backup to cloud storage
- Sync between machines
- Team collaboration

**Usage Statistics:**
- Project size
- Execution count
- Package usage
- Performance metrics
