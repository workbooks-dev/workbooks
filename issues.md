# Tether App Issues

## MAJOR logging issue:
- [x] exposes secrets in logs - FIXED: Removed all env var injection logging entirely to prevent any potential secret exposure (engine_server.py:244, 266)

## Build issues
- [x] Python engine_server setup fails. Shouldn't the engine server be running based on a `uv sync` within a project directory? Is that not the process on boot? - FIXED: Created dedicated engine venv at ~/.tether/engine/.venv with auto-sync on boot (engine_http.rs:302-389, engine_pyproject.toml, tauri.conf.json:36-38)