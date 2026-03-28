# Release wb CLI

Release a new version of the wb CLI. Handles version bump, local build verification, git tag, push, and monitors the GitHub Actions release pipeline.

## Steps

1. **Read current version** from `Cargo.toml`
2. **Determine new version**:
   - If the user provided a version argument like `/release 0.2.0`, use that
   - Otherwise, auto-increment the patch version (e.g., 0.1.0 -> 0.1.1)
3. **Update version** in `Cargo.toml`
4. **Build locally** to verify it compiles: `cargo build --release`
5. **Run tests** to verify nothing is broken: `cargo test`
6. **Run examples** as a smoke test: `./target/release/wb examples/hello.md`
7. **Verify the binary reports the new version**: `./target/release/wb version`
8. **Check git status** — warn if there are uncommitted changes beyond the version bump
9. **Commit** the version bump: `git add Cargo.toml Cargo.lock && git commit -m "Release vX.Y.Z"`
10. **Tag** the release: `git tag vX.Y.Z`
11. **Push** the commit and tag: `git push && git push --tags`
12. **Monitor GitHub Actions** — watch the release workflow:
    - `gh run list --workflow=release.yml --limit=1`
    - Wait for it to complete (check every 30s, up to 10 minutes)
    - If it fails, show the logs: `gh run view <id> --log-failed`
13. **Verify the release** exists on GitHub: `gh release view vX.Y.Z`
14. **Test the update mechanism** from a temp directory:
    - Verify the release assets are downloadable
    - `curl -fsSL https://api.github.com/repos/workbooks-dev/workbooks/releases/latest | grep tag_name`
15. **Print summary**:
    - New version
    - Release URL
    - Asset list
    - Install command: `curl -fsSL https://get.workbooks.dev | sh`

## Important

- ALWAYS build and test locally before pushing the tag
- NEVER push a tag if tests fail
- If GitHub Actions fails, diagnose and fix before retrying — do NOT just re-push the same tag
- Ask the user before pushing if there are unexpected uncommitted changes
