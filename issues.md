# Tether App Issues

## Build issues
- [ ] Drag n drop fails: Failed to save item: Command plugin:fs|stat not allowed by ACL - FIXED: Moved file type detection and reading to Rust backend (handle_dropped_item command). Frontend now only passes the path, Rust handles all fs operations. Removed overly permissive /** scope - now using minimal ACL (fs.rs:450-477, lib.rs:389-397, App.jsx:6,125-137, capabilities/default.json) - awaiting test



## React build
- [ ] might want to look into solving this during the tauri app build
    ```bash
    (!) Some chunks are larger than 500 kB after minification. Consider:
    - Using dynamic import() to code-split the application
    - Use build.rollupOptions.output.manualChunks to improve chunking: https://rollupjs.org/configuration-options/#output-manualchunks
    - Adjust chunk size limit for this warning via build.chunkSizeWarningLimit.
    ```