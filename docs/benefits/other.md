# Other Benefits

Non-obvious features that make Workbooks better than traditional notebook tools.

## HTTP-Based Engine Architecture

Each workbook runs its own Jupyter kernel managed by a local FastAPI server. The Rust backend communicates with engines over HTTP, providing clean isolation and graceful lifecycle management.

**Why this matters:** Kernels can't crash the UI. If an engine dies, just reconnect. No shared state between workbooks.

**Implementation:** `engine_server.py` runs FastAPI on port 8765; `engine_http.rs` wraps HTTP calls in Tauri commands; each workbook path maps to an independent `AsyncKernelManager`.

## Centralized Virtual Environments

Project venvs are stored in `~/.workbooks/venvs/{project-name}` instead of `.venv` in each project. This keeps venvs machine-specific and out of version control by default.

**Why this matters:** Clone a project and Workbooks automatically sets up the Python environment without polluting the project folder. No accidental venv commits.

**Implementation:** `python.rs` hashes project path to generate unique venv location; UV installs packages to centralized location; Jupyter kernel specs point to centralized Python.

## Machine-Local Secrets

Secrets are stored in `~/.workbooks/secrets/{project-hash}/secrets.db`, encrypted with AES-256-GCM. The encryption key is stored in macOS Keychain or equivalent platform keychain. Secrets never touch the project folder.

**Why this matters:** Multiple team members can have different secrets for the same project. Secrets can't accidentally be committed to git.

**Implementation:** `secrets.rs:71-100` creates project-specific secrets directory; `secrets.rs:113-149` integrates with OS keychain; `local_auth_macos.rs` provides macOS-specific authentication.

## Independent Kernel Per Workbook

Each open workbook gets its own Jupyter kernel. Variables and state don't leak between notebooks. You can restart one kernel without affecting others.

**Why this matters:** Run multiple experiments in parallel without namespace collisions. Debug one notebook without breaking others.

**Implementation:** `engine_server.py:30` maintains `engines: Dict[str, AsyncKernelManager]` mapping workbook paths to kernels; each `start_engine` call creates a new kernel.

## Autocomplete from Live Kernel

Monaco editor requests completions directly from your running Jupyter kernel, not static analysis. Autocomplete knows about variables you've defined, imported modules, and dataframe columns.

**Why this matters:** Context-aware suggestions based on actual runtime state. Type `df.` and see the actual columns from your loaded DataFrame.

**Implementation:** `WorkbookViewer.jsx:480-522` registers Monaco completion provider; `engine_server.py:722-771` forwards completion requests to kernel's `complete()` method; kernel inspects live namespace.

## Environment Variables Injected at Kernel Start

Environment variables (including secrets) are injected when the Jupyter kernel starts, not just in the shell. This means they're available to Python subprocess calls and libraries that read from `os.environ`.

**Why this matters:** Secrets work even with libraries that spawn subprocesses. `WORKBOOKS_PROJECT_FOLDER` is always set correctly.

**Implementation:** `engine_server.py:258-269` merges env vars into kernel launch environment; kernel spec includes `env` dict; kernel process inherits all variables.

## Session-Based Authentication

After authenticating once (via biometrics or password), secrets remain unlocked for 10 minutes. No repeated auth prompts during active work.

**Why this matters:** Security without constant interruption. Lock manually when stepping away.

**Implementation:** `secrets.rs:26-59` tracks session state with timeout; `secrets.rs:39-48` validates session freshness; manual lock available.

## Output Limiting Without Loss

Backend limits outputs to first 100 messages, but execution continues in background. Frontend shows truncation notice and keeps the cell running. Kernel state is preserved.

**Why this matters:** Long-running cells with excessive output don't freeze the UI. Cell finishes successfully even after truncation.

**Implementation:** `engine_server.py:514` sets `MAX_OUTPUT_MESSAGES`; `engine_server.py:622-634` sends truncation message and skips future outputs; kernel execution continues until idle state.

## Tab Restoration Per Project

When you reopen a project, Workbooks restores the tabs you had open last time. Each project remembers its own tab layout independently.

**Why this matters:** Pick up exactly where you left off. No need to reopen the same notebooks every time.

**Implementation:** `App.jsx:175-184` saves tabs to localStorage keyed by project root; `App.jsx:287-299` restores tabs on project load.

## Standard Jupyter Kernel Protocol

Workbooks uses the standard Jupyter kernel protocol, not a custom fork. Any Jupyter kernel works (Python, R, Julia, etc). Uses `jupyter_client.AsyncKernelManager` directly.

**Why this matters:** Full compatibility with Jupyter ecosystem. Install a language kernel and it just works.

**Implementation:** `engine_server.py:256` creates `AsyncKernelManager` with kernel name; `engine_server.py:329-330` uses standard Jupyter message protocol; no custom protocol extensions.

## Unsaved Changes Detection

Workbooks tracks unsaved changes per tab and shows a dot indicator. Cmd+W prompts if you want to save. Closing the app with unsaved changes asks first.

**Why this matters:** Never lose work by accident. Visual indicator shows which tabs need saving.

**Implementation:** `WorkbookViewer.jsx:847-888` maintains `hasUnsavedChanges` state; `App.jsx:191-242` checks changes before close; dialog prompts for save.

## No Hidden State Files

Workbooks stores very little metadata. Notebooks remain standard `.ipynb` files with optional timing metadata. No `.workbooks` folder in your project (except secrets, which are machine-local in `~/.workbooks`).

**Why this matters:** Projects stay clean. Share notebooks without sharing Workbooks-specific files.

**Implementation:** All Workbooks state is either in standard notebook metadata or in `~/.workbooks` outside the project.
