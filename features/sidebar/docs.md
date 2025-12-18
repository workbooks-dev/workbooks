# Sidebar

## Overview

The sidebar provides quick access to everything in the project, organized by purpose. It's a vertical navigation panel on the left side of the app.

## Structure

The sidebar has **four main sections** plus a settings button at the bottom:

### 📓 Workbooks Section

**Purpose:** Quick access to Jupyter notebooks

**Display:**
- List of `.ipynb` files from `/notebooks` folder
- Alphabetically sorted within the last 5 recently used workbooks
- Click workbook name → Opens in tab
- Click "Workbooks" header → Opens full table view in new tab
- "+ New Workbook" button at bottom

**Recent-use ordering:**
- Tracks last 20 opened workbooks
- Shows most recent 5 at top (alphabetically sorted)
- Older workbooks appear below, also alphabetically sorted

### 🔐 Secrets Section

**Purpose:** Manage encrypted credentials and API keys

**Display:**
- Lock icon header
- Placeholder text "Secrets" (not yet implemented)
- Will show list of secret keys when implemented
- Click header → Opens secrets management tab

**Future:**
- List of secret names (values hidden)
- "+ Add Secret" button
- Click secret → Edit/delete

### ⏰ Schedule Section

**Purpose:** View scheduled workbooks and run history

**Display:**
- Clock icon header
- Placeholder text "Schedule" (not yet implemented)
- Click header → Opens schedule tab with two sub-tabs

**Future tabs:**
1. **Scheduled Workbooks** - List of automated workbooks with next run time
2. **Recent Runs** - Last 30 runs with status and reports

### 📁 Files Section

**Purpose:** Access all non-notebook project files

**Display:**
- Tree view of project files
- Excludes `.ipynb` files (shown in Workbooks section)
- Click file → Opens in tab
- Right-click → Context menu (rename, delete, duplicate)
- Shows actual folder structure

**File types:**
- Data files (.csv, .json, .xlsx, .sqlite, etc.)
- Python scripts (.py)
- Markdown docs (.md)
- Any other project files

### ⚙️ Project Settings

**Location:** Gear icon at bottom of sidebar

**Purpose:** Project configuration

**Action:** Click → Opens settings tab

**Future settings:**
- Edit project name
- Manage Python packages
- Export project as ZIP

## Design Philosophy

- **Task-oriented:** Organized by what users want to do, not technical structure
- **Recent-first:** Most used items float to the top
- **Quick access:** Common actions visible without drilling down
- **Expandable:** Headers can open full views in tabs for power users
