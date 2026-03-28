# Push CLI Release

Commit, push, tag, build, and publish the Yard CLI binary to Cloudflare R2. Triggers the `cli-release.yml` GitHub Action which cross-compiles for 4 platforms and uploads binaries + install script.

## Instructions

### Step 1 — Pre-flight checks

1. Run these commands in parallel:
   - `git status` to check for uncommitted CLI changes
   - `git tag -l 'cli/v*' --sort=-v:refversion | head -5` to find the latest CLI version tag
   - `cd cli && CGO_ENABLED=0 go build -o /dev/null . && echo "build ok"` to verify the CLI compiles
   - `cd backend && uv run pytest tests/test_cli_api.py -x -q` to verify backend CLI tests pass

2. If the CLI doesn't compile or backend tests fail, stop and report the error.

### Step 2 — Commit and push pending changes

3. If there are uncommitted changes in `cli/`, `backend/yard/api/cli_*.py`, or `cli/install-yard.sh`:
   - Stage the relevant files
   - Commit with a descriptive message
   - Push to `main` (or the current branch)

4. If there are no uncommitted changes, skip to Step 3.

### Step 3 — Determine version

5. Parse the latest `cli/v*` tag to find the current version.
   - If no tags exist, suggest `v0.0.1`.
   - Otherwise, suggest bumping the patch version (e.g., `v0.0.1` -> `v0.0.2`).

6. Ask the user to confirm or provide a different version:
   ```
   Latest CLI tag: cli/v0.0.1
   Suggested next: cli/v0.0.2

   Enter version (or press enter for v0.0.2):
   ```

### Step 4 — Tag and push

7. Ensure the current branch is up to date with remote:
   ```
   git fetch origin main
   git status
   ```
   - If behind remote, warn the user.

8. Create the tag on the current HEAD:
   ```
   git tag cli/<version>
   ```

9. Push only the tag (not the branch):
   ```
   git push origin cli/<version>
   ```

### Step 5 — Monitor the build

10. Find the triggered workflow run:
    ```
    gh run list --workflow=cli-release.yml --limit=1 --json databaseId,status,conclusion,headBranch
    ```

11. Watch it until completion (timeout 10 minutes):
    ```
    gh run watch <RUN_ID> --exit-status
    ```

12. If the run fails:
    - Show failed logs: `gh run view <RUN_ID> --log-failed`
    - Stop and report the failure.

### Step 6 — Verify

13. Verify the release was created and binaries attached:
    ```
    gh release view cli/<version>
    ```

14. Report the final status:
    ```
    ## CLI Release: cli/<version>

    Tag: cli/<version>
    Platforms: linux/amd64, linux/arm64, darwin/amd64, darwin/arm64
    GitHub Release: <release URL>

    Install on a VPS:
      curl -fsSL https://get.paracord.co/yard | bash
      # or specific version:
      curl -fsSL https://get.paracord.co/yard | bash -s -- <version>
    ```

## Prerequisites

Before the first release, the user must manually:
1. Create the `yard-cli` R2 bucket in Cloudflare
2. Configure R2 public access (Worker at `yard-cli.infra-443.workers.dev`)
3. Add GitHub repo secrets: `CF_ACCOUNT_ID`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`
