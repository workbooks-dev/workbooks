# Network - To Do

## Network Status Indicator

- [ ] Create NetworkStatus component
  - [ ] Display in top-right corner or status bar
  - [ ] States: Online (green), Offline (red), Checking (yellow)
  - [ ] Icon or badge indicator
  - [ ] Tooltip explaining current state

- [ ] Connection detection
  - [ ] Periodic network checks
  - [ ] Ping PyPI or astral.sh
  - [ ] Quick timeout (1-2 seconds)
  - [ ] Cache result briefly (30-60 seconds)
  - [ ] Update status indicator

## Error Messages

- [ ] Offline error dialogs
  - [ ] "Cannot create project" dialog
  - [ ] "Cannot install packages" dialog
  - [ ] "Cannot complete setup" dialog
  - [ ] Clear, actionable error messages
  - [ ] Retry button
  - [ ] Cancel button

- [ ] Error handling in operations
  - [ ] Check network before starting
  - [ ] Early failure with clear message
  - [ ] Suggest connecting to internet
  - [ ] Provide retry option

## Progress Indicators

- [ ] Download progress UI
  - [ ] Show what's being downloaded
  - [ ] Progress bar (if available)
  - [ ] Download size/speed
  - [ ] Estimated time remaining
  - [ ] Cancel button

- [ ] Operation status messages
  - [ ] "Installing uv..."
  - [ ] "Installing Python 3.12..."
  - [ ] "Installing [package]..."
  - [ ] "Updating dependencies..."
  - [ ] Success messages
  - [ ] Real-time status updates

## Retry Mechanism

- [ ] Automatic retries
  - [ ] Retry network errors 3 times
  - [ ] Exponential backoff
  - [ ] Show retry count to user
  - [ ] Give up after max retries

- [ ] Manual retry
  - [ ] Retry button in error dialogs
  - [ ] Re-attempt failed operation
  - [ ] Resume from checkpoint if possible
  - [ ] Clear previous error state

## Network Checks

- [ ] Pre-operation connectivity check
  - [ ] Before creating project
  - [ ] Before installing packages
  - [ ] Before downloading uv
  - [ ] Quick fail if offline

- [ ] Graceful degradation
  - [ ] Disable features requiring network when offline
  - [ ] Show disabled state in UI
  - [ ] Re-enable when connection restored

## User Education

- [ ] First-run explanation
  - [ ] Tooltip or dialog on first launch
  - [ ] Explain internet requirement for setup
  - [ ] Set expectations

- [ ] Offline capabilities guide
  - [ ] In-app help section
  - [ ] List what works offline
  - [ ] List what requires internet
  - [ ] Troubleshooting tips

## Status Messages

- [ ] Standardize status message format
  - [ ] Clear operation names
  - [ ] Consistent success messages
  - [ ] Consistent error messages
  - [ ] Progress indicators where applicable

## Implementation

- [ ] Backend network checking
  - [ ] Rust function to check connectivity
  - [ ] Tauri command: `check_network_status()`
  - [ ] Return online/offline/unknown

- [ ] Frontend integration
  - [ ] Poll network status
  - [ ] Update UI state
  - [ ] Disable/enable features based on status
  - [ ] Show appropriate messages

## Testing

- [ ] Test offline behavior
  - [ ] Disconnect network and verify error messages
  - [ ] Verify retry mechanism
  - [ ] Verify online/offline transitions
  - [ ] Test all network-dependent operations

- [ ] Test error scenarios
  - [ ] Network timeout
  - [ ] Connection lost during download
  - [ ] DNS failure
  - [ ] PyPI unavailable

## Future Enhancements

- [ ] Offline package cache
  - [ ] Cache downloaded packages
  - [ ] Reuse across projects
  - [ ] Reduce re-downloads

- [ ] Pre-download for offline use
  - [ ] "Download for offline" option
  - [ ] Pre-fetch common packages
  - [ ] Background download

- [ ] Resume interrupted downloads
  - [ ] Save partial downloads
  - [ ] Resume from last byte
  - [ ] Handle connection drops gracefully
