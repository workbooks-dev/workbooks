# Project Settings

## Overview

Project settings provide configuration and management for the entire Tether project. Accessed via the gear icon at the bottom of the sidebar.

**See `/project-defaults.md` for default packages and project initialization.**

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
- Updates `.tether` shortcut file
- Updates window title
- Validation (no special chars)

**Project Path:**
- Read-only display
- Absolute path to project root
- Copy button for convenience

**Created Date:**
- When project was initialized
- Read-only

#### 2. Python Environment

**Virtual Environment:**
- Shows venv path (centralized: `~/.tether/venvs/{project-name}-{hash}`)
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
  - Encrypted secrets (`.env.tether`)
  - Project structure
- Excludes:
  - `.tether/` directory (state, runs, etc.)
  - Virtual environment
  - Python cache files

**Recipient Setup:**
- Open exported ZIP in Tether
- Tether recreates venv and installs packages
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
- Tether integration: cloudpickle (for state)

**Customization (Future):**
- Edit default package list
- Create project templates
- Different defaults for different types of projects

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

**`.tether/project.json`:**
```json
{
  "name": "My Project",
  "created_at": 1234567890,
  "project_root": "/Users/name/Projects/my-project",
  "python_version": "3.12",
  "venv_path": "/Users/name/.tether/venvs/my-project-abc123"
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
├── .env.tether (encrypted secrets)
└── My Project.tether (shortcut file)
```

**Import Flow:**
1. User extracts ZIP (or Tether does it)
2. Tether detects `.tether` shortcut file
3. Opens as new project
4. Creates venv at `~/.tether/venvs/{name}-{hash}`
5. Runs `uv sync` to install packages
6. Prompts for secrets (if `.env.tether` exists)
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
