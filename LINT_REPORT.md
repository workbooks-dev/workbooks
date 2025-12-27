# Linting Results Summary

Generated: 2025-12-27

## JavaScript/React - ESLint

**Total: 50 problems (4 errors, 46 warnings)**

### Critical Errors (Must Fix)

1. **AiChatPanel.jsx:382** - `assistantMessageIndex` is not defined
2. **AiSidebar.jsx:725** - Parsing error: Expecting Unicode escape sequence
3. **Sidebar.jsx:516** - Unexpected lexical declaration in case block
4. **WorkbookViewer.jsx:693** - Unexpected control character in regex

### Unused Variables/Imports (Can Clean Up)

- **App.jsx:5** - `ask` imported but never used
- **App.jsx:193** - `name` assigned but never used
- **Canvas.jsx:18** - `setNodes` assigned but never used
- **ClaudeApprovalModal.jsx:35** - `changesByTool` assigned but never used
- **FileViewer.jsx:547** - `arr` parameter defined but never used
- **ScheduleTab.jsx:5** - `onClose` parameter defined but never used
- **Sidebar.jsx:470-471** - `filteredFiles`, `displayFiles` assigned but never used
- **Sidebar.jsx:508** - `extraData` parameter defined but never used
- **WorkbookViewer.jsx:177** - `onInsertBelow`, `autosaveEnabled` parameters not used
- **WorkbookViewer.jsx:666** - `zoomedImage`, `setZoomedImage` assigned but never used
- **WorkbookViewer.jsx:906** - `cellExecutionStartTime` assigned but never used
- **WorkbookViewer.jsx:909** - `engineReady` assigned but never used
- **WorkbookViewer.jsx:911** - `cellRefs` assigned but never used
- **WorkbookViewer.jsx:913** - `executingCellRef` assigned but never used
- **WorkbookViewer.jsx:971** - `err` variable defined but never used
- **WorkbookViewer.jsx:1119** - `restartErr` defined but never used
- **WorkbookViewer.jsx:1123** - `stopErr` defined but never used
- **WorkbookViewer.jsx:1912** - `handleClose` assigned but never used

### React Hooks Dependencies (31 warnings)

Multiple components have incomplete dependency arrays in `useEffect` hooks. While these work, they may cause stale closures or unexpected behavior. Review each case to determine if dependencies should be added or if the warning can be safely ignored.

## Python - Ruff

**Issues found:**

### Import Sorting (Auto-fixable)
- Multiple files have unsorted imports (I001)
- Run `npm run lint:py:fix` to auto-fix

### Deprecated Type Annotations
- **config.py:4** - `typing.Dict` is deprecated, use `dict` instead (UP035)
- **config.py:23, 26** - Using `Dict` instead of `dict` in type annotations (UP006)

**Auto-fix:** Run `npm run lint:py:fix` to automatically fix all these issues.

## Rust - Clippy

**Issues found:**

### Configuration Warnings
Multiple warnings about unexpected `cfg` condition value in `local_auth_macos.rs` related to the `objc` crate. These are not code quality issues but rather build configuration warnings.

**Fix:** These can be suppressed by adding to the top of `local_auth_macos.rs`:
```rust
#![allow(unexpected_cfgs)]
```

Or update the `objc` dependency to a newer version.

## Quick Wins - What to Fix First

### 1. Fix JavaScript Errors (Required)
```bash
# These are breaking errors that need immediate attention
# - Fix undefined variable in AiChatPanel.jsx
# - Fix parsing error in AiSidebar.jsx
# - Fix switch statement in Sidebar.jsx
# - Fix regex in WorkbookViewer.jsx
```

### 2. Auto-fix Python Issues
```bash
npm run lint:py:fix
npm run format:py
```

### 3. Remove Obvious Unused Variables
Start with the easy ones:
- Remove unused imports in App.jsx
- Remove `setNodes` in Canvas.jsx
- Remove unused variables in WorkbookViewer.jsx
- Prefix intentionally unused parameters with `_`

## How to Auto-Fix

```bash
# JavaScript - auto-fix what can be fixed
npm run lint:js:fix

# Python - auto-fix all issues
npm run lint:py:fix
npm run format:py

# Rust - fix manually based on warnings
cd src-tauri && cargo clippy
```

## Regular Maintenance

**Weekly:**
```bash
npm run lint
```

**Before commits:**
```bash
npm run lint:js:fix
npm run lint:py:fix
git add .
```

**For VS Code users:**
- Install recommended extensions (will be prompted)
- Issues will show inline with squiggly underlines
- Many issues auto-fix on save

## Next Steps

1. Fix the 4 critical JavaScript errors first
2. Run auto-fix for Python: `npm run lint:py:fix`
3. Review and remove unused variables in JavaScript
4. Decide on React hooks dependency warnings (case by case)
5. Set up pre-commit hooks (optional, see LINTING.md)
