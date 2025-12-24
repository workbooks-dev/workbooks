# Workbooks Application Dependencies

This document tracks all dependencies across the Workbooks application stack.

## Python Dependencies

Managed via `pyproject.toml` and `uv`.

### Core Runtime Dependencies (Current)
```toml
[project.dependencies]
fastapi = ">=0.124.0"
uvicorn = ">=0.38.0"
jupyter-client = ">=8.6.0"
requests = ">=2.32.5"
```

### Missing Dependency (BUG)
- **pydantic** - Used in engine_server.py for request/response models but NOT in pyproject.toml!

### Dependency Purposes
- **fastapi** - HTTP server for engine lifecycle management (engine_server.py)
- **uvicorn** - ASGI server to run FastAPI
- **jupyter-client** - Jupyter kernel/engine management via AsyncKernelManager
- **requests** - HTTP client library
- **pydantic** - Data validation and request/response models (MISSING FROM DEPS)

### Per-Project Dependencies
Created via template in `src-tauri/src/project.rs:262`:
```toml
[dependency-groups]
workbooks = [
    "pip>=24.0.0",
    "jupyter>=1.0.0",
    "jupyter-client>=8.0.0",
    "ipykernel>=6.0.0",
]
```

### Package Installation Method
- Uses `uv pip install` command (see `src-tauri/src/python.rs:216,236`)
- NOT using `uv add` - using legacy pip interface

### Planned Additions
- `workbooks-core` - Python package for state API (`from workbooks import state`)
- `nbformat` - Parse and manipulate .ipynb files
- `cloudpickle` - Serialize Python objects for checkpointing
- `sqlalchemy` - For state.db interaction
- `boto3` - Optional S3 sync
- `croniter` - Cron scheduling (future)

---

## JavaScript/React Dependencies

Managed via `package.json` and `npm`.

### Core Framework
```json
{
  "react": "^19.1.0",
  "react-dom": "^19.1.0"
}
```

### UI Components & Editors
```json
{
  "@monaco-editor/react": "^4.7.0",
  "@xyflow/react": "^12.10.0",
  "marked": "^17.0.1",
  "react-markdown": "^10.1.0",
  "react-syntax-highlighter": "^16.1.0",
  "rehype-raw": "^7.0.0"
}
```

#### Purposes
- **@monaco-editor/react** - Code editor (cell editing, file viewer)
- **@xyflow/react** - Visual pipeline canvas (INSTALLED, NOT YET USED)
- **marked** - Markdown parsing
- **react-markdown** - Markdown rendering with React
- **react-syntax-highlighter** - Code highlighting in markdown cells
- **rehype-raw** - Allow raw HTML in markdown

### Tauri Integration
```json
{
  "@tauri-apps/api": "^2",
  "@tauri-apps/plugin-dialog": "^2.4.2",
  "@tauri-apps/plugin-opener": "^2"
}
```

#### Purposes
- **@tauri-apps/api** - Core Tauri API (invoke, events, etc.)
- **@tauri-apps/plugin-dialog** - File/folder pickers, native dialogs
- **@tauri-apps/plugin-opener** - Open files with default apps

### Build Tools & Styling (DevDependencies)
```json
{
  "@tailwindcss/typography": "^0.5.19",
  "@tauri-apps/cli": "^2",
  "@vitejs/plugin-react": "^4.6.0",
  "autoprefixer": "^10.4.23",
  "postcss": "^8.5.6",
  "tailwindcss": "^3.4.19",
  "vite": "^7.0.4"
}
```

#### Purposes
- **@tauri-apps/cli** - Tauri build and dev tooling
- **vite** - Frontend build tool with hot reload
- **@vitejs/plugin-react** - React support for Vite
- **tailwindcss** - Utility-first CSS framework
- **@tailwindcss/typography** - Typography plugin for prose content
- **autoprefixer** - Auto-add CSS vendor prefixes
- **postcss** - CSS transformation tool

### Removed Dependencies
- ESLint and related plugins were removed from current package.json
- Type definitions for React were removed (using JSX, not TypeScript)

### Planned Additions
- None currently planned

---

## Rust/Tauri Dependencies

Managed via `Cargo.toml` in `src-tauri/`.

### Core Tauri Framework
```toml
[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-dialog = "2"
tauri-plugin-opener = "2"
tauri-plugin-window-state = "2"
```

### Serialization & Data
```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

### Async Runtime
```toml
tokio = { version = "1", features = ["full"] }
futures-util = "0.3"
```

### HTTP Client
```toml
reqwest = { version = "0.12", features = ["blocking", "json", "stream"] }
```

### Error Handling & Utilities
```toml
anyhow = "1"
once_cell = "1"
```

### System & Process
```toml
which = "4"
dirs = "5"
```

### Jupyter Protocol (ZMQ Integration)
```toml
zmq = "0.10"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
hmac = "0.12"
sha2 = "0.10"
hex = "0.4"
```

### Build Dependencies
```toml
[build-dependencies]
tauri-build = { version = "2", features = [] }
```

### Dependency Purposes
- **tauri** - Core framework for desktop app
- **tauri-plugin-dialog** - File/folder pickers
- **tauri-plugin-opener** - Open files with default apps
- **tauri-plugin-window-state** - Save/restore window state
- **serde/serde_json** - Serialization for Tauri commands
- **tokio** - Async runtime for HTTP requests and process management
- **futures-util** - Async stream utilities
- **reqwest** - HTTP client for engine server communication
- **anyhow** - Error handling with context
- **once_cell** - Lazy static initialization
- **which** - Find executables in PATH (for uv)
- **dirs** - Get system directories (home, etc.)
- **zmq** - ZeroMQ for direct Jupyter kernel communication (src/kernel.rs)
- **uuid** - Generate message IDs for Jupyter protocol
- **chrono** - Timestamps for Jupyter messages
- **hmac/sha2/hex** - Message signing for Jupyter protocol

### Current Architecture
- HTTP-based engine communication is primary (reqwest → engine_server.py)
- Direct ZMQ integration exists in `src/kernel.rs` but not currently used
- Both approaches are available, HTTP is currently active

### Planned Additions
- `rusqlite` - SQLite integration for state.db (future)

---

## System Dependencies

### Bundled/Managed by App
- **uv** - Python package manager (bundled with Tauri, auto-installed if missing)

### Required at Runtime
- **Python 3.8+** - Managed by uv per-project
- **WebView2** (Windows), **WebKit** (macOS/Linux) - Provided by Tauri

### Optional
- **SQLite** - For state.db (can use rusqlite or Python sqlite3)
- **Git** - For version control (not required by app)

---

## Dependency Update Strategy

### Python
```bash
# Update all Python dependencies
uv lock --upgrade

# Add new dependency
uv add <package>

# Remove dependency
uv remove <package>
```

### JavaScript
```bash
# Update all JS dependencies
npm update

# Add new dependency
npm install <package>

# Add dev dependency
npm install --save-dev <package>
```

### Rust
```bash
# Update all Rust dependencies
cd src-tauri
cargo update

# Add new dependency (edit Cargo.toml or use cargo-edit)
cargo add <crate>
```

---

## Version Pinning Policy

- **Python**: Use `>=` for flexibility, lock with `uv.lock`
- **JavaScript**: Use `^` for semver compatibility
- **Rust**: Use semver ranges in Cargo.toml, lock with Cargo.lock

---

## Security Updates

All dependencies should be reviewed quarterly for security updates:
```bash
# Python
uv lock --upgrade

# JavaScript
npm audit
npm audit fix

# Rust
cargo audit  # Requires: cargo install cargo-audit
```

---

## License Compatibility

All dependencies are compatible with MIT license (Workbooks's planned license):
- Permissive licenses: MIT, Apache-2.0, BSD
- Check any new dependencies before adding
