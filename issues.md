
## Tether APp
- [x] When opening a previous project, restore previous state (which files were open)
- [x] When creating a new notebook, automatically open that notebook
- [x] When closing the app, if a file is dirty, it must prevent app closing and prompt user to save any dirty files.


## Notebooks
- [x] The count for cell execution is still incorrect. *any* execution should increment the cell execution count. Empty cells *do not* execute anything therefor it's not an execution. Markdown does not execute anything either.

## Autocomplete
- [x] How do we add autocomplete so the typing is far better?
- [x] Autocomplete is hidden or just plain doesn't work. 


## Notebook Engine (Jupyter Kernel)
- [x] Ports are not being closed, maybe a clean up process is needed on boot (assuming these ports were initiated by a dead tether process)



## Virtual environments
- [ ] Let's change the location of virtual environments so it doesn't muddy up the current folder. Let's have something like "~/.tether/venvs" that keeps track of all tether project virtual environments. We can also delete individual venvs that haven't been in use for a while and "sync" projects when they start (I think it already does that).