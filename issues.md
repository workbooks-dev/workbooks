
## Tether App

## Notebooks
- [x] Fresh project (empty folder) cannot create new notebooks
    [Log] Folder opened: – Object (Welcome.jsx, line 41)
    Object
    [Log] Initializing Python environment... (Welcome.jsx, line 43)
    [Log] Python environment initialized (Welcome.jsx, line 47)
    [Error] Failed to create workbook: – "Failed to write workbook file"
        (anonymous function) (FileExplorer.jsx:227)
    **Fixed:** Added directory creation in create_workbook (src-tauri/src/fs.rs:69-73)

- [x] cannot close window error Unhandled Promise Rejection: window.destroy not allowed. Permissions associated with this command: core:window:allow-destroy
    **Fixed:** Added core:window:allow-destroy permission (src-tauri/capabilities/default.json:8)

- [x] Fresh project does not create the uv pyproject file with the standard dependencies (project-defaults.md)
    **Fixed:** Modified open_folder to initialize uv project with tether dependencies (src-tauri/src/project.rs:121-128)

- [x] Fresh project, engine does not connect right away. if necessary, show the engine state
    **Already implemented:** Engine status indicator exists in WorkbookViewer (src/components/WorkbookViewer.jsx:596, :1422-1428)



## Autocomplete


## Notebook Engine (Jupyter Kernel)


## Virtual environments
