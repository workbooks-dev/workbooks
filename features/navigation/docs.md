# Navigation System

## Overview

Tether uses a **tab-based navigation system** for all views. Instead of modals or separate windows, everything opens as a new tab in the main workspace.

## Design Principle

**Everything is a tab:**
- Opening a workbook → New tab
- Viewing all workbooks (table view) → New tab
- Managing secrets → New tab
- Editing project settings → New tab
- Viewing schedule/run history → New tab
- Opening regular files → New tab

## Current Implementation

**App.jsx** manages the tab system with:
- `tabs` state - Array of open tabs
- `activeTab` state - Currently visible tab
- Tab types: `workbook`, `file`, `welcome`, `create`
- Tab close functionality
- Active tab highlighting

**TabBar.jsx** component displays:
- List of open tabs with icons
- Close buttons (×) for each tab
- Active tab indicator
- Autosave toggle control

## Tab Data Structure

```javascript
{
  id: "unique-id",
  type: "workbook" | "file" | "welcome" | "create" | "workbooks-table" | "secrets" | "settings",
  title: "Display Name",
  path: "/path/to/file", // for workbook/file tabs
  unsaved: false // dirty state indicator
}
```

## User Experience

- Click item in sidebar → Opens in new tab (or switches if already open)
- Multiple files can be open simultaneously
- Tabs persist during session (not between app restarts)
- Closing last tab shows welcome screen
