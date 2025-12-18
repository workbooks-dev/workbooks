
## Tether APp


## Notebooks
- [ ] cell execution order has unexpected behavior
- [ ] execution once again is not using what's currently displayed but some sort of cache or stored data. I noticed this when I did a "Run All" command
- [ ] Every file save says it was modified outside of Tether, that's not true

## Autocomplete


## Notebook Engine (Jupyter Kernel)


## Virtual environments
- [x] Let's change the location of virtual environments so it doesn't muddy up the current folder. Let's have something like "~/.tether/venvs" that keeps track of all tether project virtual environments. We can also delete individual venvs that haven't been in use for a while and "sync" projects when they start (I think it already does that).
  - **COMPLETED**: Virtual environments are now stored in `~/.tether/venvs/<project-name>-<hash>`
  - Each project gets a unique venv based on its package name and path hash
  - Projects sync dependencies when opened using `uv sync` with `UV_PROJECT_ENVIRONMENT` env var
  - Project folders remain clean and shareable (only pyproject.toml and uv.lock needed)
  - Dependencies from `[dependency-groups.tether]` are automatically installed on project open/create