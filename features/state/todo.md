# State - To Do

## Python Package (workbooks-core)

- [ ] Create workbooks-core Python package
  - [ ] Package structure and setup.py
  - [ ] PyPI publication (future)

- [ ] StateManager class
  - [ ] `get(key, default, wait, timeout)` implementation
  - [ ] `set(key, value, metadata)` implementation
  - [ ] `delete(key)` implementation
  - [ ] `list()` implementation
  - [ ] `watch(key, callback)` implementation

- [ ] Storage backend
  - [ ] SQLite connection management
  - [ ] Blob storage (cloudpickle)
  - [ ] Size threshold (inline vs blob)
  - [ ] Automatic serialization
  - [ ] Error handling

- [ ] Context variables
  - [ ] `state.run_id` - Current run identifier
  - [ ] `state.workbook_name` - Current workbook path
  - [ ] Inject during execution

## Backend (Rust)

- [ ] State storage system
  - [ ] SQLite database setup
  - [ ] Schema creation (state, dependencies tables)
  - [ ] Blob directory management
  - [ ] File locking and concurrency

- [ ] StateManager implementation
  - [ ] `get_state(key)` command
  - [ ] `set_state(key, value)` command
  - [ ] `list_state()` command
  - [ ] `delete_state(key)` command
  - [ ] `get_state_metadata(key)` command

- [ ] Dependency tracking
  - [ ] `track_state_access(workbook, key, type)` command
  - [ ] `get_workbook_dependencies(workbook)` command
  - [ ] `get_state_dependencies(key)` command
  - [ ] Build dependency graph

- [ ] Size management
  - [ ] Detect value size
  - [ ] Store inline if small (<1KB)
  - [ ] Store as blob if large (>=1KB)
  - [ ] Automatic threshold handling

## Checkpointing System

- [ ] Checkpoint creation
  - [ ] Save namespace before each cell
  - [ ] Filter to picklable objects
  - [ ] Store in `.workbooks/runs/{run_id}/checkpoints/`
  - [ ] Chain cell hashes for invalidation

- [ ] Resume functionality
  - [ ] Detect interrupted runs
  - [ ] Load latest valid checkpoint
  - [ ] Restore namespace
  - [ ] Skip executed cells
  - [ ] Continue execution

- [ ] Checkpoint management
  - [ ] Cleanup old checkpoints
  - [ ] Configurable retention
  - [ ] Disk space monitoring

## State Forking (Future)

- [ ] Branch management
  - [ ] `fork(branch_name)` - Create branch
  - [ ] `switch(branch_name)` - Switch to branch
  - [ ] `list_branches()` - Show all branches
  - [ ] `delete_branch(branch_name)` - Delete branch

- [ ] Copy-on-write
  - [ ] Copy state.db to branches/
  - [ ] Copy state/ blobs to branches/
  - [ ] Track current branch
  - [ ] Swap databases on switch

- [ ] Merge functionality (Advanced)
  - [ ] Compare branches
  - [ ] Conflict resolution
  - [ ] Merge strategies

## Frontend (StatePanel)

- [ ] StatePanel component
  - [ ] Table view of all state variables
  - [ ] Columns: Key, Type, Size, Last Modified, Created By
  - [ ] Search/filter functionality
  - [ ] Click to inspect value

- [ ] State inspector
  - [ ] Show value details
  - [ ] DataFrame preview
  - [ ] JSON viewer
  - [ ] Download state variable

- [ ] Delete functionality
  - [ ] Delete button per state variable
  - [ ] Confirmation dialog
  - [ ] Warning if still referenced

- [ ] Dependency visualization
  - [ ] Show which workbooks read/write each state
  - [ ] Dependency graph (React Flow)
  - [ ] Click to navigate to workbook

## Integration

- [ ] Inject workbooks-core into kernels
  - [ ] Add to project dependencies
  - [ ] Auto-import in kernel startup
  - [ ] Configure state storage path

- [ ] Execution integration
  - [ ] Track state access during execution
  - [ ] Update dependency graph
  - [ ] Trigger dependent workbooks (future)

- [ ] Sidebar integration
  - [ ] State section (optional)
  - [ ] Show state count
  - [ ] Click to open StatePanel tab

## Dependency Graph

- [ ] Auto-discovery
  - [ ] Parse cells for state.get/set calls
  - [ ] Extract state keys
  - [ ] Build dependency graph
  - [ ] Update on cell execution

- [ ] Visualization (React Flow)
  - [ ] Node per workbook
  - [ ] Edge per state dependency
  - [ ] Color-code by freshness
  - [ ] Interactive navigation

- [ ] Execution order
  - [ ] Topological sort
  - [ ] Determine run order
  - [ ] Parallel execution where possible

## Scheduler Integration

- [ ] State-triggered execution
  - [ ] Run workbook when dependencies updated
  - [ ] Reactive scheduling
  - [ ] Configurable triggers

- [ ] Workflow example
  - [ ] load_data → writes customers
  - [ ] transform → reads customers, writes customers_clean
  - [ ] train_model → reads customers_clean, writes model
  - [ ] Auto-cascade on schedule

## Testing

- [ ] Test state CRUD operations
- [ ] Test blob storage
- [ ] Test dependency tracking
- [ ] Test checkpointing and resume
- [ ] Test state forking (when implemented)
- [ ] Performance tests for large state

## Documentation

- [ ] User guide for state API
- [ ] Examples and tutorials
- [ ] Best practices
- [ ] Migration guide for existing notebooks
