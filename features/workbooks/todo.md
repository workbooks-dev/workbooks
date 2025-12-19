# Workbooks - To Do


## High Priority
- [ ] Moving cells has an issue. Moving a cell up or down does not update the ui display, the cell is techincally moving (verified by saving + reload the page) but the UI is broken and not showing the element actually move.
- [ ] Markdown image display, is `![image.png]($TETHER_PROJECT_FOLDER / “image.png”)` possible? If not, how do we show images in markdown from a local folder?

## Medium Priority

- [ ] Output improvements:
  - [ ] Interactive widget support (ipywidgets)
  - [ ] Plotly/Bokeh chart support

- [ ] Cell profiling and performance:
  - [ ] Memory usage tracking per cell
  - [ ] CPU time profiling
  - [ ] Performance breakdown visualization

- [ ] Execution state on tab changes should persist (it currently reverts to saved state)

- [ ] Hovering above or below cells should prompt the option to add a cell (code or markdown)


## Low Priority

- [ ] Cell folding/collapsing for long code
- [ ] Split view for comparing workbooks
- [ ] Cell comments/annotations
- [ ] Variable inspector panel
- [ ] Debugger integration
- [ ] Workbook templates library
- [ ] Cell timing profiler
