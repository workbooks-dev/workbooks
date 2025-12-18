# Tether Features Documentation

This directory contains feature-by-feature documentation, todos, and completion tracking for the Tether application.

## Structure

Each feature has its own directory with three files:

- **`docs.md`** - What the feature is, how it works, design decisions
- **`todo.md`** - What needs to be implemented
- **`done.md`** - What has been completed

## Feature Areas

### Core UI
- **`navigation/`** - Tab-based navigation system
- **`sidebar/`** - Sidebar structure and sections (Workbooks, Secrets, Schedule, Files)

### Workbook System
- **`workbooks/`** - Workbook viewer, execution engine, keyboard shortcuts, streaming output
- **`files/`** - File management, environment variables, file drop handling

### Data & Security
- **`secrets/`** - Secrets management, encryption, keychain integration
- **`state/`** - State management system (SQLite, blob storage, tether-core API)

### Automation
- **`schedule/`** - Cron scheduling, run history, automated execution

### Configuration
- **`project-settings/`** - Project settings, package management, export
- **`network/`** - Network requirements, offline behavior, status indicators

## Top-Level Files

- **`todo.md`** - High-level roadmap and cross-cutting todos
- **`changelog.md`** - Chronological list of completed work
- **`README.md`** - This file

## Workflow

When implementing a feature:

1. Read `features/<area>/docs.md` to understand the design
2. Check `features/<area>/todo.md` for what needs to be done
3. Implement the feature
4. Move completed items from `todo.md` → `done.md`
5. Add entry to `features/changelog.md` with date and description

## Benefits

- **Focused development** - Work on one feature without context-switching
- **Clear progress tracking** - Easy to see what's done vs pending
- **Better AI context** - Claude can read just the relevant docs
- **Git-friendly** - Fewer merge conflicts, clearer diffs
- **Scalable** - Add new feature areas as needed
