# Project Settings - To Do

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
