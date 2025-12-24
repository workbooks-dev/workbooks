# Marketplace - Todo

## Backend (Rust)

### GitHub Integration
- [ ] Create GitHub API client for fetching templates
- [ ] Implement template metadata parsing (template.json)
- [ ] Download template files to project directory
- [ ] Handle dependency file copying (pyproject.toml or requirements.txt)
- [ ] Merge dependencies into existing project pyproject.toml
- [ ] Run `uv sync` after template installation
- [ ] Cache template index locally for offline access

### Secret Detection
- [ ] Parse notebooks for `os.environ["KEY"]` and `os.getenv("KEY")` patterns
- [ ] Cross-reference with project secrets
- [ ] Return list of missing secrets for UI display
- [ ] Add notebook metadata tracking for secret warnings

## Frontend (React)

### Marketplace UI
- [ ] Add "Marketplace" section to sidebar (above Files, below Schedule)
- [ ] Create MarketplaceView component with Official/Community tabs
- [ ] Template grid/list view with search and filtering
- [ ] Template detail modal/view with README preview
- [ ] "Add to Project" button and installation flow
- [ ] Dependency merge confirmation dialog
- [ ] Loading states and error handling
- [ ] Secret warning badge/indicator for notebooks with missing secrets

### Template Browser
- [ ] Category filtering (Cloud Storage, ETL, Notifications, etc.)
- [ ] Search by name/tags/description
- [ ] Template cards showing name, author, description, tags
- [ ] Notebook count indicator
- [ ] Dependency preview

## Infrastructure

### Template Repositories
- [ ] Create `workbooks-dev/templates-official` repo
- [ ] Create `workbooks-dev/templates-community` repo
- [ ] Set up repo structure and README
- [ ] Create first official template (AWS S3 Sync)
- [ ] Document template contribution guidelines

### Documentation
- [ ] Add marketplace usage guide to main docs
- [ ] Template creation guide for contributors
- [ ] Security review process documentation 