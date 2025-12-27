# Code Quality & Unused Code Detection

This document explains how to detect and prune unused code across the Workbooks codebase.

## Quick Start

```bash
# Install linting dependencies
npm install
uv sync

# Run all linters
npm run lint

# Auto-fix what can be fixed
npm run lint:js:fix
npm run lint:py:fix
```

## Tools Overview

### JavaScript/React - ESLint
- **Config**: `eslint.config.js`
- **Detects**: Unused variables, unused imports, React hooks issues
- **LSP**: Works with VS Code ESLint extension

```bash
# Check for issues
npm run lint:js

# Auto-fix issues
npm run lint:js:fix
```

### Python - Ruff
- **Config**: `ruff.toml`
- **Detects**: Unused imports (F401), unused variables (F841), code quality issues
- **LSP**: Built-in LSP support for real-time feedback in editors
- **Why Ruff?**: 10-100x faster than traditional tools, written in Rust, excellent LSP

```bash
# Check for issues
npm run lint:py

# Auto-fix issues
npm run lint:py:fix

# Format code
npm run format:py

# Check formatting
npm run format:check:py
```

#### Setting up Ruff LSP in VS Code

Add to `.vscode/settings.json`:
```json
{
  "[python]": {
    "editor.defaultFormatter": "charliermarsh.ruff",
    "editor.formatOnSave": true,
    "editor.codeActionsOnSave": {
      "source.fixAll": "explicit",
      "source.organizeImports": "explicit"
    }
  },
  "ruff.nativeServer": true
}
```

Install the Ruff extension: `charliermarsh.ruff`

### Rust - Clippy
- **Config**: `.clippy.toml`
- **Detects**: Unused code (compiler warns by default), code quality issues
- **LSP**: Built into rust-analyzer

```bash
# Check for issues (strict mode - treats warnings as errors)
npm run lint:rust

# Or run directly
cd src-tauri
cargo clippy -- -D warnings

# Check without failing on warnings
cargo clippy
```

## Regular Pruning Workflow

### 1. Weekly/Monthly Code Review

```bash
# Run all linters to get a full report
npm run lint

# Review warnings and decide what to keep/remove
# Focus on:
# - F401: Unused imports
# - F841: Unused variables
# - no-unused-vars: Unused JS variables
```

### 2. Before Commits

Consider adding a pre-commit hook. Create `.git/hooks/pre-commit`:

```bash
#!/bin/bash
echo "Running linters..."

# JavaScript
npm run lint:js
if [ $? -ne 0 ]; then
  echo "❌ ESLint failed. Fix issues or use --no-verify to skip."
  exit 1
fi

# Python
npm run lint:py
if [ $? -ne 0 ]; then
  echo "❌ Ruff failed. Fix issues or use --no-verify to skip."
  exit 1
fi

# Rust (warnings don't block, only errors)
cd src-tauri && cargo clippy --quiet
if [ $? -ne 0 ]; then
  echo "❌ Clippy failed. Fix issues or use --no-verify to skip."
  exit 1
fi

echo "✅ All linters passed!"
```

Make it executable:
```bash
chmod +x .git/hooks/pre-commit
```

### 3. CI/CD Integration

Add to your GitHub Actions workflow:

```yaml
- name: Lint JavaScript
  run: npm run lint:js

- name: Lint Python
  run: npm run lint:py

- name: Lint Rust
  run: cd src-tauri && cargo clippy -- -D warnings
```

## Common Unused Code Patterns

### JavaScript/React

**Unused imports:**
```javascript
import { useState } from 'react'; // ❌ Imported but never used
import { invoke } from '@tauri-apps/api/core'; // ❌ Imported but never used

export function MyComponent() {
  return <div>Hello</div>;
}
```

**Unused variables:**
```javascript
export function MyComponent() {
  const [data, setData] = useState(null); // ❌ 'data' never used
  const unusedVar = 42; // ❌ Variable declared but never used

  return <div>Hello</div>;
}
```

**Fix with prefix:**
```javascript
// If you need to keep the variable for future use:
const _unusedVar = 42; // ✅ Prefixed with _ to indicate intentionally unused
```

### Python

**Unused imports:**
```python
from fastapi import FastAPI  # ❌ F401: Imported but never used
import sys  # ❌ F401: Imported but never used

def main():
    print("Hello")
```

**Unused variables:**
```python
def process_data(data):
    result = transform(data)  # ❌ F841: Local variable assigned but never used
    filtered = filter_data(data)  # ❌ F841: Local variable assigned but never used
    return data
```

**Fix with prefix:**
```python
# If you need to keep the variable for type checking or clarity:
def process_data(data):
    _result = transform(data)  # ✅ Intentionally unused
    return data
```

### Rust

**Unused code:**
```rust
fn unused_function() {  // ⚠️ Warning: function is never used
    println!("Hello");
}

pub fn my_function() {
    let unused_var = 42;  // ⚠️ Warning: unused variable
}
```

**Fix:**
```rust
// Remove entirely, or prefix with underscore:
pub fn my_function() {
    let _unused_var = 42;  // ✅ Intentionally unused
}
```

## Ignoring False Positives

### JavaScript
```javascript
// eslint-disable-next-line no-unused-vars
const keepThis = 42;
```

### Python
```python
# ruff: noqa: F841
keep_this = 42

# Or for entire file:
# ruff: noqa
```

### Rust
```rust
#[allow(dead_code)]
fn keep_this() {
    // ...
}
```

## Tips for Keeping Code Clean

1. **Run linters frequently** - Catch unused code early before it accumulates
2. **Use auto-fix** - Many issues can be fixed automatically
3. **Review before deleting** - Some "unused" code might be needed for future features
4. **Document intentional unused code** - Use `_` prefix or comments to explain why
5. **Clean up during refactoring** - When changing features, remove related unused code
6. **Check imports** - Remove unused imports first (easiest wins)

## Editor Integration

### VS Code

Install extensions:
- **ESLint** (`dbaeumer.vscode-eslint`)
- **Ruff** (`charliermarsh.ruff`)
- **rust-analyzer** (`rust-lang.rust-analyzer`)

All three will show unused code warnings in real-time with squiggly underlines.

### Other Editors

- **Neovim**: Use nvim-lspconfig with eslint, ruff-lsp, and rust-analyzer
- **Sublime**: Use LSP package with appropriate language servers
- **IntelliJ/WebStorm**: Built-in support for all three

## Measuring Progress

Track unused code over time:

```bash
# Count JavaScript warnings
npm run lint:js 2>&1 | grep "warning" | wc -l

# Count Python issues
npm run lint:py 2>&1 | grep -E "F401|F841" | wc -l

# Count Rust warnings
cd src-tauri && cargo clippy 2>&1 | grep "warning:" | wc -l
```

## Resources

- [ESLint Rules](https://eslint.org/docs/latest/rules/)
- [Ruff Rules](https://docs.astral.sh/ruff/rules/)
- [Clippy Lints](https://rust-lang.github.io/rust-clippy/master/)
