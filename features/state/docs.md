# State Management System

## Overview

Workbooks's state system allows workbooks to share data through a persistent, durable key-value store. This enables workbook orchestration without explicit wiring.

**Status:** Not yet implemented. This is a future feature.

## Design Philosophy

**Implicit Connections:**
- Workbooks communicate through shared state, not explicit links
- Dependencies inferred automatically from `state.get()` and `state.set()` calls
- No manual DAG definition required

**Durable by Default:**
- All state persisted to SQLite
- Large objects stored as pickled blobs
- Survives app restarts
- Checkpoint-based execution

**State-Based Orchestration:**
- Workbook runs when dependencies available
- Blocks on missing state or uses cached
- Automatic dependency tracking

## User Experience

### Python API

```python
from workbooks import state

# Read from state (blocks until available or uses cached)
customers_df = state.get("customers")

# Write to state
state.set("customers_clean", df_clean)

# List all state keys
all_state = state.list()

# Delete state
state.delete("old_data")

# Watch for changes
def on_change(key, value):
    print(f"{key} was updated!")

state.watch("customers", on_change)

# Access run context
print(state.run_id)  # Current run ID
print(state.workbook_name)  # Current workbook name
```

### StatePanel Component (Future)

**Sidebar or Tab:**
- Shows all state variables
- Key, type, size, last modified
- Click to inspect value
- Delete state variables
- Track dependencies

**Visualization:**
- Show which workbooks read/write each state
- Dependency graph
- Data lineage

## Technical Architecture

### Storage Layer

**SQLite Database:**
- `.workbooks/state.db` - Metadata and small values
- Index on keys for fast lookups
- Tracks creation/modification times

**Blob Storage:**
- `.workbooks/state/` - Directory for large pickled objects
- Filenames: `{key}.pkl`
- Uses `cloudpickle` for serialization
- Supports DataFrames, models, complex objects

**Schema:**
```sql
CREATE TABLE state (
  key TEXT PRIMARY KEY,
  value_type TEXT NOT NULL,  -- 'inline' or 'blob'
  value TEXT,                 -- For small values
  blob_path TEXT,             -- For large values
  size INTEGER,
  created_at INTEGER NOT NULL,
  modified_at INTEGER NOT NULL,
  created_by TEXT,            -- Workbook that created it
  modified_by TEXT            -- Last workbook that modified it
);

CREATE TABLE state_dependencies (
  workbook_path TEXT NOT NULL,
  key TEXT NOT NULL,
  access_type TEXT NOT NULL,  -- 'read' or 'write'
  last_accessed INTEGER NOT NULL,
  PRIMARY KEY (workbook_path, key, access_type)
);
```

### Python Package: workbooks-core

**Location:** `workbooks-core/` (to be created)

**API Implementation:**
```python
# workbooks/state.py
class StateManager:
    def get(self, key: str, default=None, wait=False, timeout=None):
        """
        Read from state.

        Args:
            key: State variable name
            default: Value if key doesn't exist
            wait: Block until key is available
            timeout: Max wait time in seconds
        """
        pass

    def set(self, key: str, value: Any, metadata: dict = None):
        """
        Write to state.

        Args:
            key: State variable name
            value: Any picklable Python object
            metadata: Optional metadata dict
        """
        pass

    def delete(self, key: str):
        """Delete state variable."""
        pass

    def list(self) -> list[dict]:
        """List all state keys with metadata."""
        pass

    def watch(self, key: str, callback: Callable):
        """Watch for changes to a state variable."""
        pass

# Global instance
state = StateManager()
```

### Tauri Backend Integration

**Rust Commands:**
- `get_state(key)` - Retrieve from SQLite/blob
- `set_state(key, value)` - Store to SQLite/blob
- `list_state()` - Query all state keys
- `delete_state(key)` - Remove from storage
- `get_state_dependencies(workbook_path)` - Get dependencies for workbook

**Storage Manager:**
```rust
// src-tauri/src/state.rs
pub struct StateManager {
    db: SqliteConnection,
    blob_dir: PathBuf,
}

impl StateManager {
    pub fn get(&self, key: &str) -> Result<StateValue>;
    pub fn set(&self, key: &str, value: StateValue) -> Result<()>;
    pub fn delete(&self, key: &str) -> Result<()>;
    pub fn list(&self) -> Result<Vec<StateMetadata>>;
    pub fn track_access(&self, workbook: &str, key: &str, access_type: AccessType) -> Result<()>;
}
```

## Checkpointing Strategy

**Cell-by-Cell Checkpoints:**
1. Before each cell executes, save current namespace
2. Filter to picklable objects only
3. Store in `.workbooks/runs/{run_id}/checkpoints/cell-{n}.pkl`
4. On resume, load latest checkpoint and continue from next cell
5. Chain cell hashes so code changes invalidate downstream checkpoints

**Checkpoint Structure:**
```
.workbooks/runs/{run_id}/
├── checkpoints/
│   ├── cell-0.pkl
│   ├── cell-1.pkl
│   └── cell-2.pkl
├── metadata.json
└── notebook.ipynb (final state)
```

**Resume Logic:**
1. Load latest valid checkpoint
2. Restore namespace
3. Skip already-executed cells
4. Continue from next cell
5. Create new checkpoints as execution progresses

## State Forking (Neon-Style Branches)

**Use Case:**
- "What if I trained the model with different parameters?"
- Fork state, experiment, compare, merge or discard

**Implementation:**
```python
# Fork current state
state.fork("experiment-1")

# Switch to branch
state.switch("experiment-1")

# Make changes...
state.set("model", trained_model)

# Switch back to main
state.switch("main")

# Compare branches
main_accuracy = state.get("model_accuracy")
state.switch("experiment-1")
exp_accuracy = state.get("model_accuracy")

# Merge if better
if exp_accuracy > main_accuracy:
    state.merge("experiment-1", into="main")
```

**Backend:**
- Copy `state.db` to `branches/{branch_name}.db`
- Copy `state/` blobs to `branches/{branch_name}_blobs/`
- Track current branch
- Swap databases on switch

## Dependency Tracking

**Auto-Discovery:**
- Parse workbook cells for `state.get()` and `state.set()` calls
- Build dependency graph automatically
- No manual configuration

**Dependency Graph:**
```
load_data.ipynb
  └─ writes: customers (DataFrame)

transform.ipynb
  ├─ reads: customers
  └─ writes: customers_clean (DataFrame)

train_model.ipynb
  ├─ reads: customers_clean
  └─ writes: model (sklearn model)
```

**Execution Order:**
- Workbooks can determine execution order from dependencies
- Run workbooks in correct sequence
- Parallel execution where possible

## Integration with Scheduling

**State-Driven Scheduling:**
- Schedule workbook to run when dependencies are ready
- "Run `transform.ipynb` whenever `customers` is updated"
- Reactive execution based on state changes

**Workflow:**
1. `load_data.ipynb` runs on schedule (daily 9am)
2. Writes `customers` state
3. Triggers `transform.ipynb` (depends on `customers`)
4. Writes `customers_clean` state
5. Triggers `train_model.ipynb` (depends on `customers_clean`)

## Future Enhancements

**Cloud Sync:**
- Sync state to S3/R2
- Share state across machines
- Team collaboration

**State Versioning:**
- Track state history
- Rollback to previous versions
- Audit trail

**State Visualization:**
- Dependency graph viewer (React Flow)
- Data lineage tracking
- State usage analytics

**Performance:**
- Lazy loading of large state
- Compression for blobs
- Caching strategies
