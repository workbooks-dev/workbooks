# Action Window

## Overview

The Action Window is Tether's central launcher and hub interface. It serves as the primary entry point for all Tether operations, appearing when the app first opens, when users press Command+N, or when accessing features from the tray menu.

## Design Goals

1. **Universal Entry Point** - Single interface for all major Tether operations
2. **Quick Access** - Fast navigation to recent projects and common tasks
3. **Cross-Project Views** - Access to global schedule and run history across all projects
4. **Discoverable** - Clear presentation of available actions for new users
5. **Minimal Friction** - Get users to their destination in one click

## Use Cases

### Opening Tether
- User double-clicks Tether app → Action Window appears
- User clicks tray icon with no windows open → Action Window appears
- User presses Command+N in any window → New Action Window appears

### From Action Window, Users Can:
1. **Open Recent Projects** - Click on any of the 3 most recent projects
2. **Create New Project** - Start a new Tether project
3. **Open Existing Project** - Browse and open any project folder
4. **View All Runs** - See run history across all projects
5. **View All Schedules** - See scheduled workbooks across all projects
6. **Install MCPs** - Manage Model Context Protocol servers

## UI Layout

```
┌─────────────────────────────────────────────────────────┐
│                                                         │
│                    [Tether Logo]                        │
│                                                         │
│  ┌───────────────────────────────────────────────────┐  │
│  │                                                   │  │
│  │  Recent Projects                                  │  │
│  │  ┌─────────────────────────────────────────────┐ │  │
│  │  │  My Pipeline              ~/Projects/pipe   │ │  │
│  │  │  Data Analysis            ~/Work/analysis   │ │  │
│  │  │  Model Training           ~/ML/training     │ │  │
│  │  └─────────────────────────────────────────────┘ │  │
│  │                                                   │  │
│  │  Projects                                         │  │
│  │  ┌──────────────────┐  ┌──────────────────────┐ │  │
│  │  │  Create Project  │  │  Open Project...     │ │  │
│  │  └──────────────────┘  └──────────────────────┘ │  │
│  │                                                   │  │
│  │  Global Views                                     │  │
│  │  ┌──────────────────┐  ┌──────────────────────┐ │  │
│  │  │  View All Runs   │  │  View All Schedules  │ │  │
│  │  └──────────────────┘  └──────────────────────┘ │  │
│  │                                                   │  │
│  │  Settings                                         │  │
│  │  ┌──────────────────┐                            │  │
│  │  │  Install MCP...  │                            │  │
│  │  └──────────────────┘                            │  │
│  │                                                   │  │
│  └───────────────────────────────────────────────────┘  │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

## Component Structure

### Recent Projects Section
- Shows 3 most recently opened projects
- Each item displays:
  - Project name (bold)
  - Project path (muted, smaller)
- Click → Opens project in current window OR creates new project window
- Empty state: "No recent projects"

### Projects Section
- **Create Project** - Opens project creation dialog
- **Open Project** - Opens native file picker to select project folder

### Global Views Section
- **View All Runs** - Opens a global run history view across all projects
  - Shows runs from all projects in chronological order
  - Filterable by project, status, date range
- **View All Schedules** - Opens global scheduler view
  - Shows all scheduled workbooks across all projects
  - Displays next run time, frequency, status

### Settings Section
- **Install MCP** - Opens MCP management interface
  - Browse available MCPs
  - Install from directory
  - Manage installed MCPs

## Behavior

### Window Management
- Action Window is a normal Tauri window (not a dialog)
- Multiple Action Windows can be open simultaneously
- Command+N from any window → Opens new Action Window
- Closing Action Window doesn't quit app (tray keeps running)

### Navigation Flow
When user clicks an action:
1. **Recent Project / Open Project**
   - If project already open in a window → Focus that window
   - Otherwise → Transform current Action Window into Project Window with that project
2. **Create Project**
   - Show create project dialog
   - On success → Transform Action Window into new Project Window
3. **View All Runs / View All Schedules**
   - Transform Action Window into global view mode
   - These are special "projectless" views showing cross-project data
4. **Install MCP**
   - Open new dedicated MCP management window
   - Keep Action Window open

### State Persistence
- Recent projects loaded from `~/.tether/recent_projects.json`
- Window size/position preferences stored in app state
- Last viewed section remembered (if applicable)

## Implementation Details

### Location
- `src/components/ActionWindow.jsx` - Main Action Window component
- `src-tauri/src/recent_projects.rs` - Recent projects tracking
- `src-tauri/src/lib.rs` - Tauri commands for action window operations

### Key Tauri Commands
```rust
#[tauri::command]
async fn get_recent_projects() -> Result<Vec<RecentProject>, String>

#[tauri::command]
async fn open_project_picker() -> Result<String, String>

#[tauri::command]
async fn get_all_runs() -> Result<Vec<Run>, String>

#[tauri::command]
async fn get_all_schedules() -> Result<Vec<Schedule>, String>
```

### Keyboard Shortcuts
- **Command+N** - Open new Action Window (global)
- **Command+O** - Open project picker (when Action Window focused)
- **Command+1/2/3** - Open recent project 1/2/3 (when Action Window focused)
- **Escape** - Close Action Window (if not the last window)

## Visual Design

### Style
- Clean, centered layout with generous whitespace
- Card-based sections with subtle borders
- Consistent with overall Tether aesthetic (see STYLE_GUIDE.md)
- Logo/branding at top center
- Maximum width container (e.g., 800px) for readability

### Colors
- Grayscale + Blue accents (consistent with app theme)
- Hover states on all interactive elements
- Subtle shadows on cards for depth
- Status indicators use semantic colors (green/amber/red)

### Typography
- Large, clear section headers
- Project names in medium weight
- Paths and metadata in muted text
- Action buttons with clear labels

## Integration with Tray

The Action Window and Tray Menu share functionality:
- Both access recent projects
- Both provide create/open project actions
- Both can navigate to global views
- Both access MCP management

Tray menu items that open views will:
1. Look for existing Action Window
2. If found → Use that window for navigation
3. If not found → Create new Action Window and navigate

## Future Enhancements

- Quick search/command palette (Command+K)
- Project templates on creation
- Pinned/favorite projects (separate from recent)
- Quick stats: "3 runs today", "2 scheduled jobs"
- Onboarding flow for first-time users
- Recent activity feed
- Global notifications/alerts
