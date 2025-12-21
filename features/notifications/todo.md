# Notifications - TODO

## Backend (Rust)

- [ ] Create `src-tauri/src/notifications.rs` module
- [ ] Implement `NotificationManager` struct with SQLite storage
- [ ] Create notification database schema and migrations
- [ ] Implement notification creation methods (run success/failure, errors, updates)
- [ ] Implement notification query methods (list, unread count, recent)
- [ ] Implement notification update methods (mark read, dismiss, clear)
- [ ] Add auto-pruning of old notifications (30+ days)
- [ ] Add NotificationManager to AppState
- [ ] Register notification Tauri commands in lib.rs
- [ ] Integrate with scheduler (notify on run completion/failure)
- [ ] Integrate with engine (notify on execution errors)
- [ ] Add `tauri-plugin-notification` dependency for OS notifications
- [ ] Implement OS notification sending (with user preferences)
- [ ] Add notification settings to config/preferences system

## Frontend (React)

- [ ] Create `NotificationCenter.jsx` component (slide-out panel)
- [ ] Create `NotificationBadge.jsx` component (toolbar bell icon with count)
- [ ] Create `NotificationItem.jsx` component (individual notification display)
- [ ] Add notification icon/badge to main toolbar
- [ ] Implement notification list with filtering by type
- [ ] Implement mark as read/unread functionality
- [ ] Implement dismiss and clear all functionality
- [ ] Add click handlers to navigate to relevant context (run, workbook, etc.)
- [ ] Add notification event listener to update badge count in real-time
- [ ] Style notifications with colors/icons by type
- [ ] Add empty state when no notifications

## Tray Integration

- [ ] Add unread notification count to tray menu
- [ ] Add "Recent Notifications" submenu to tray
- [ ] Update tray when new notifications arrive
- [ ] Add click handlers in tray to open app and navigate to notification context
- [ ] Add "View All Notifications" tray menu item

## Settings/Preferences

- [ ] Add notification preferences section in Settings
- [ ] Enable/disable OS notifications toggle
- [ ] Choose which event types trigger notifications
- [ ] Notification retention period setting (7/14/30 days)

## Testing

- [ ] Test notification creation from scheduler
- [ ] Test notification creation from engine errors
- [ ] Test notification list pagination
- [ ] Test mark read/unread
- [ ] Test dismiss and clear all
- [ ] Test notification badge count updates
- [ ] Test OS notifications on macOS
- [ ] Test notification pruning (old notification cleanup)
- [ ] Test tray notification display

## Documentation

- [ ] Add notification system to user guide
- [ ] Document notification types and their triggers
- [ ] Document notification settings/preferences
