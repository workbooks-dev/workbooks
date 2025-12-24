# Project Settings - Completed

## Sidebar Integration

- [x] Gear icon at bottom of sidebar
- [x] Click handler in place
- [x] Visual positioning

## Backend Foundation

- [x] Project creation with `pyproject.toml`
- [x] Virtual environment management
- [x] Package installation via uv
- [x] Dependency syncing
- [x] `.workbooks` shortcut file generation

## Python/uv Integration

- [x] `install_python_package()` command
- [x] `install_python_packages()` command
- [x] `sync_dependencies()` - Sync from pyproject.toml
- [x] Centralized venv location (`~/.workbooks/venvs/`)

## Design Updates

- [x] Designed global configuration system (`~/.workbooks/config.toml`)
- [x] Designed default project feature (shared with CLI)
- [x] Designed CLI integration section for settings
- [x] Designed Claude Desktop integration ("Add to Claude" button)
- [x] Designed "Manage Claude Projects" feature
- [x] Designed CLI installation UI and workflow
- [x] Planned Tauri installer with opt-out CLI installation

## Notes

**No settings UI built yet.** Gear icon exists but settings tab component needs to be created. See `todo.md` for implementation roadmap.
