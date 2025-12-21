# Marketplace

A curated marketplace of notebook templates to jumpstart common workflows.

## Overview

The Marketplace provides users with pre-built notebook templates for common tasks like cloud storage sync, data transformations, API integrations, etc. Templates are normal Jupyter notebooks with optional metadata and dependencies.

## Architecture

### Two-Repo Model

1. **Official Templates** (`tether-dev/templates-official`)
   - Curated, reviewed templates maintained by Tether team
   - High quality, well-documented, secure
   - Examples: AWS S3 sync, Postgres ETL, Slack notifications

2. **Community Templates** (`tether-dev/templates-community`)
   - User-contributed templates via PR
   - Reviewed for malicious code but not quality-guaranteed
   - Wider variety, experimental workflows

### Template Structure

```
template-name/
├── template.json         # Metadata (name, description, author, tags, version)
├── notebook.ipynb        # The workbook (can be multiple notebooks)
├── README.md            # Documentation, usage instructions
└── pyproject.toml       # Python dependencies (or requirements.txt)
```

### Template Metadata Format

```json
{
  "name": "AWS S3 Sync",
  "description": "Sync files between local storage and S3",
  "author": "Tether",
  "category": "Cloud Storage",
  "tags": ["aws", "s3", "sync", "cloud"],
  "version": "1.0.0",
  "notebooks": ["sync.ipynb"],
  "dependencies": "pyproject.toml"
}
```

## Secret Detection

Templates are normal notebooks - **no special placeholder injection system**.

Instead, Tether automatically detects missing secrets:

1. Parse notebook for `os.environ["KEY"]` or `os.getenv("KEY")` usage
2. Check if each KEY exists in project secrets
3. Show warning badge: "⚠️ Missing secrets" with list
4. User adds secrets manually via Secrets UI

**Templates themselves cannot add/remove secrets** - they just reference env vars that users must configure.

## UI Flow

1. User clicks **"Marketplace"** in sidebar (above Files, below Schedule)
2. Shows two tabs: "Official" and "Community"
3. Grid/list view with search and category filtering
4. Click template → detail view with:
   - Description, author, tags
   - README preview
   - List of notebooks included
   - Dependencies list
   - **"Add to Project"** button
5. On add:
   - Download template notebooks to project
   - Copy pyproject.toml dependencies (or prompt to merge)
   - Run `uv sync` to install dependencies
   - Open first notebook
   - Show secret detection warning if needed

## Dependency Handling

Templates can specify dependencies via:
- `pyproject.toml` (preferred)
- `requirements.txt` (legacy support)

On template install:
1. If project has no `pyproject.toml`, copy template's dependencies file
2. If project has existing `pyproject.toml`, prompt user:
   - "Merge dependencies from template?"
   - Show diff of what will be added
   - User approves/rejects
3. Run `uv sync` to install

## Future Enhancements (Not v1)

- Template updates: track source repo/version, notify on updates
- Ratings/reviews for community templates
- Template versioning (user chooses version to install)
- Private template repos (enterprise use case)
- Template creation wizard in app
- Direct URL import: `tether install https://github.com/user/template`

## Security & Moderation

**Official repo:**
- All templates reviewed by maintainers
- Code review on every PR
- Run automated security scans

**Community repo:**
- PR review for malicious code only (not quality)
- Clear disclaimer: "Community templates are user-contributed. Review code before use."
- Potentially add reporting mechanism later

## Implementation Notes

- Marketplace data fetched from GitHub API (repos and releases)
- Templates cached locally for offline access after first fetch
- No template update mechanism in v1 (may track source for future)
- Secret detection happens on notebook open, not template install
