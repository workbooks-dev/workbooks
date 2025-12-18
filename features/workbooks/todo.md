# Workbooks - To Do

## High Priority

- [ ] Lock icon when secrets are active
  - [ ] Show lock icon in WorkbookViewer when secrets system is enabled
  - [ ] Indicate which secrets are available to current workbook

- [ ] Secret detection dialog
  - [ ] Scan cell code before execution for hardcoded secrets
  - [ ] Prompt user to move to secrets manager
  - [ ] Auto-rewrite cell to use `os.environ["SECRET_NAME"]`

- [ ] Output redaction integration
  - [ ] Scan outputs for secret values before saving
  - [ ] Replace with `[REDACTED]` or similar
  - [ ] Prevent accidental secret commits

## Medium Priority

- [ ] Persist execution metadata:
  - [ ] Last run time per cell
  - [ ] Execution duration per cell
  - [ ] Total workbook run time
  - [ ] Store in `.tether/` metadata database

- [ ] Cell execution status indicators:
  - [ ] Show execution count `[3]` like Jupyter
  - [ ] Show running indicator during execution
  - [ ] Show error indicator on failed cells

- [ ] Output improvements:
  - [ ] Better HTML/DataFrame rendering
  - [ ] Interactive widget support
  - [ ] Plotly/Bokeh chart support
  - [ ] Image zoom/lightbox

- [ ] Execution queue:
  - [ ] "Run All" button to execute all cells sequentially
  - [ ] "Run All Above" / "Run All Below" options
  - [ ] Queue visualization

## Low Priority

- [ ] Cell folding/collapsing for long code
- [ ] Split view for comparing workbooks
- [ ] Cell comments/annotations
- [ ] Variable inspector panel
- [ ] Debugger integration
- [ ] Workbook templates library
- [ ] Cell timing profiler
