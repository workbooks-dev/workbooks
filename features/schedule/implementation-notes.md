# Schedule Implementation Notes

## Exploration Findings (Dec 19, 2025)

### Current State

**CLI:**
- No CLI binary exists yet
- Current Cargo.toml only defines a library (`tether_lib`)
- No CLI argument parsing library (clap/structopt) in dependencies

**Engine Capabilities:**
- Engine uses FastAPI server (`engine_server.py`) on HTTP
- Current endpoints:
  - `/engine/start` - Start Jupyter kernel for workbook
  - `/engine/execute` - Execute single cell
  - `/engine/execute-stream` - Execute cell with streaming output
  - `/engine/complete` - Code completion
  - `/engine/stop` - Stop engine
  - `/engine/interrupt` - Interrupt execution
- **Missing:** No endpoint to execute all cells in a workbook sequentially

**Architecture:**
- Each workbook gets its own AsyncKernelManager
- Engines managed via HTTP calls from Rust → FastAPI → Jupyter kernel
- Streaming outputs supported via callback to Rust → Tauri event emission

### Implementation Plan

#### 1. Add Execute All Cells Capability

**Engine Server (`engine_server.py`):**
- Add `/engine/execute-all` endpoint
- Accept workbook path + notebook content (cells array)
- Execute cells sequentially
- Return combined results with individual cell outputs
- Support streaming mode for progress updates

#### 2. Create CLI Binary

**Structure:**
```
src-tauri/
├── src/
│   ├── main.rs          # GUI app entry point (existing)
│   ├── cli.rs           # CLI entry point (new)
│   └── lib.rs           # Shared library
```

**Cargo.toml:**
```toml
[[bin]]
name = "tether"          # CLI binary
path = "src/cli.rs"

[[bin]]
name = "tether-gui"      # GUI binary
path = "src/main.rs"
```

**Dependencies to add:**
- `clap` with derive feature for CLI parsing
- `tokio-cron-scheduler` for cron scheduling

#### 3. Global Scheduler Database

**Location:** `~/.tether/schedules.db`

**Schema:**
```sql
CREATE TABLE schedules (
  id TEXT PRIMARY KEY,
  workbook_path TEXT NOT NULL,
  project_root TEXT NOT NULL,
  cron_expression TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL,
  modified_at INTEGER NOT NULL,
  next_run INTEGER,
  last_run INTEGER,
  UNIQUE(workbook_path, project_root)
);

CREATE TABLE runs (
  id TEXT PRIMARY KEY,
  schedule_id TEXT,
  workbook_path TEXT NOT NULL,
  project_root TEXT NOT NULL,
  started_at INTEGER NOT NULL,
  finished_at INTEGER,
  duration INTEGER,
  status TEXT NOT NULL,
  error_message TEXT,
  report_path TEXT,
  FOREIGN KEY (schedule_id) REFERENCES schedules(id)
);
```

#### 4. Scheduler Module

**New File:** `src-tauri/src/scheduler.rs`

**Components:**
- `SchedulerManager` - Main scheduler state
- `Schedule` struct - Individual schedule
- `Run` struct - Execution record
- Functions:
  - `add_schedule(workbook, project, cron)`
  - `list_schedules()`
  - `delete_schedule(id)`
  - `update_schedule(id, cron, enabled)`
  - `get_next_run(schedule_id)`
  - Background task runner with tokio-cron-scheduler

#### 5. CLI Commands

**`tether run <notebook>`:**
- Load notebook from path
- Start engine for notebook
- Execute all cells sequentially
- Print outputs to stdout
- Exit with success/error code

**`tether schedule <notebook> --cron <expr>`:**
- Add schedule to global database
- Validate cron expression
- Print next run time
- Note: Scheduler runs when GUI app is open

**`tether schedule <notebook> --daily`:**
- Preset for "0 9 * * *" (9am daily)

**`tether schedule <notebook> --hourly`:**
- Preset for "0 * * * *" (top of every hour)

**`tether schedule list`:**
- List all schedules
- Show next run times

**`tether schedule remove <schedule-id>`:**
- Remove schedule from database

#### 6. Integration with GUI

**AppState updates:**
```rust
pub struct AppState {
    pub current_project: Mutex<Option<TetherProject>>,
    pub engine_server: Arc<Mutex<Option<EngineServer>>>,
    pub secrets_manager: Arc<Mutex<Option<SecretsManager>>>,
    pub scheduler: Arc<Mutex<SchedulerManager>>,  // NEW
}
```

**Scheduler lifecycle:**
- Start scheduler background task when GUI app opens
- Load schedules from global database
- Execute scheduled runs using engine server
- Pause when app closes

### Technical Decisions

**Why global schedules?**
- Sharing a project shouldn't auto-schedule workbooks
- MCP server can see all scheduled items across projects
- Single source of truth for automation
- Easier to manage and debug

**Why tokio-cron-scheduler?**
- Pure Rust implementation
- Async/await support (works with Tokio runtime)
- Good cron expression parsing
- Active maintenance

**Why keep GUI and CLI separate?**
- Different use cases (interactive vs automation)
- CLI can run without GUI overhead
- Easier testing and deployment

### Next Steps

1. ✅ Document findings
2. Add `/engine/execute-all` endpoint to `engine_server.py`
3. Update `Cargo.toml` for CLI binary and new dependencies
4. Create `scheduler.rs` module
5. Create `cli.rs` for CLI entry point
6. Test with sample workbook
7. Update features/schedule/done.md
