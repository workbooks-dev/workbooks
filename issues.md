# Tether App Issues


## Build issues
- [] Tether should open 1920 x 1080 if first open and it's possible.
- [x] shouldn't tether automatically solve issues like this? in some cases uv will be found in `.local/bin/uv` in other cases tether will need to fix it. uv was installed but not found in PATH. You may need to restart your terminal or add ~/.cargo/bin to your PATH
- [x] App hangs at "starting" - Fixed by disabling tauri-plugin-window-state which was causing startup hangs (lib.rs:940-941)
- [ ] Workbook hangs at "Starting..." with no indication. 
- [ ] We need runtime logs as a native file menu drop down


## UX issues
- [ ] command+Q does not quit the application
- [ ] command+w after no tabs, does not close the project window (as it should)
- [ ] When closing on a dirty file, we should have "Save + close", "Don't save and close" and "Cancel"