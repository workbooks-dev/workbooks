# Workbooks

Run markdown as code.

## What It Is

`wb` is a CLI that executes fenced code blocks in markdown files. Write docs that run. Share runbooks that actually work.

```
wb run deploy-check.md -o results.md
```

## Tech Stack

- **Rust** — single binary, ~650KB, zero runtime dependencies
- **clap** — CLI argument parsing
- **serde + serde_yaml** — frontmatter parsing
- **chrono** — timestamps

## Project Structure

```
workbooks/
├── Cargo.toml
├── install.sh         # curl | sh installer
├── src/
│   ├── main.rs        # CLI entrypoint (wb run, wb inspect)
│   ├── parser.rs      # Markdown frontmatter + code block extraction
│   ├── executor.rs    # Multi-runtime subprocess execution
│   ├── checkpoint.rs  # Save/resume execution state
│   ├── pending.rs     # Pending-signal descriptors for paused workbooks (`wait`)
│   ├── callback.rs    # HTTP webhook notifications with HMAC signing
│   ├── secrets.rs     # Secret providers (doppler, yard, env, dotenv, prompt)
│   └── output.rs      # Results markdown formatter
└── examples/
    ├── hello.md
    ├── health-check.md
    ├── data-pipeline.md
    ├── multi-runtime.md
    ├── secrets-demo.md
    └── deploy-check.md
```

## Workbook Format

Markdown files with optional YAML frontmatter:

```markdown
---
title: My Workbook
runtime: python
venv: ./.venv
secrets:
  provider: doppler
  project: my-project
---

# My Workbook

```bash
echo "runs in bash"
```

```python
print("runs in python")
```
```

- Frontmatter configures runtime, venv, secrets, env vars
- Code block language tag determines which runtime executes it
- Non-executable blocks (yaml, json, etc.) are preserved as documentation

## Per-block timeouts, retries, and continue-on-error

Frontmatter maps keyed by **1-based block number** (matching the `[N/total]` UI):

```yaml
---
timeouts:
  1: 30s              # override block 1's timeout (default 300s)
  3: 2m
retries:
  3: 2                # retry block 3 up to 2 more times on failure
continue_on_error: [4] # block 4 failure doesn't trigger --bail
---
```

- **`timeouts`** — values are duration strings (`30s`, `5m`, `2h`, bare int = seconds). A timed-out block gets `error_type: "timeout"` and `stdout_partial: true` / `stderr_partial: true` in JSON output and callback payloads — partial output is preserved so agents can diagnose hung blocks. A timeout kills the language session child; a later retry or block will spawn a fresh session (state reset).
- **`retries`** — number of *additional* attempts after the first failure (`0`/missing = no retry). Retries run with a 500ms delay between attempts. Useful for flaky HTTP calls; combine with `timeouts:` to cap individual attempts.
- **`continue_on_error`** — block numbers whose failure should not halt a `--bail` run. The block's failure is still recorded and emitted via callbacks; execution just continues to the next block.

Callback payloads (`step.complete`, `checkpoint.failed`) include `stdout_partial` / `stderr_partial` fields so downstream agents can distinguish "block failed" from "block was cut off mid-run".

## Composing workbooks with `include:`

Factor out repeated setup (logins, env priming, health pre-flights) into its own
workbook and pull it into others via an `include:` fence:

```markdown
```include
path: ./login.md
```
```

The target workbook's blocks are spliced into the parent's block list as if they'd
been written there, inheriting the parent's env + `$WB_ARTIFACTS_DIR`, so any
session/token/file the login writes is visible to downstream blocks. The included
workbook can still be run and tested on its own — `wb run login.md`.

- **Path resolution** — relative to the including workbook's directory, not the CWD.
  `./login.md` means "next to me". An included workbook that itself includes `./c.md`
  resolves `c.md` relative to *its own* directory.
- **Frontmatter precedence** — the included workbook's frontmatter is ignored. The
  parent's runtime/secrets/env/venv/timeouts/retries control the run. (Keep shared
  config in the parent; login workbooks only need blocks.)
- **Progress + checkpoints** — included blocks count toward the parent's `[N/total]`.
  Checkpoints save the parent's file + block count; if either changes (including by
  editing the included file), the run starts fresh.
- **Cycle detection** — `A → B → A` fails at load time with exit code 3. The same
  target included twice at different positions is allowed.
- **Errors** — missing target, circular include, or malformed `include:` YAML all
  exit with `EXIT_WORKBOOK_INVALID` (3) before any block runs.

See `examples/include-demo.md` + `examples/include-login.md` for a minimal example.
Passing parameters into an included workbook (beyond env vars the parent exports) is
scoped for a later milestone.

## CLI Usage

```bash
wb run file.md                        # Run and show output
wb run file.md -o results.md          # Save results as markdown
wb run file.md --bail                 # Stop on first failure
wb run file.md -q                     # Suppress block output in terminal
wb run file.md --secrets doppler      # Override secret provider
wb run file.md -C /path/to/dir        # Set working directory
wb run file.md --checkpoint my-run    # Save/resume execution state
wb run file.md --callback <url>       # POST events to webhook
wb inspect file.md                    # Show structure without running
wb pending                            # List paused workbooks (auto-reaps expired abort-mode descriptors)
wb pending --no-reap                  # List without reaping — safe for automation/inspection
wb resume <id> --signal <file>        # Resume a paused workbook with a signal payload
wb cancel <id>                        # Drop a paused workbook without resuming
```

## Pausing on external signals

Workbooks can pause on a `wait` fence until an external resolver (an agent,
webhook handler, cron job, or a human) delivers the awaited payload:

```markdown
```wait
kind: email
match:
  from: auth@example.com
bind: otp_code
timeout: 5m
on_timeout: abort
```
```

When `wb` hits a `wait` block it writes a checkpoint + a pending-signal
descriptor next to it, then exits with code **42** ("paused, not failed"). The
process is gone — nothing stays in memory.

`wb` is protocol-agnostic: `kind` and `match` are opaque metadata that external
resolvers interpret. To resume, deliver the bound value:

```bash
wb resume my-run --value 123456                      # single-bind shortcut
wb resume my-run --signal payload.json               # JSON payload
echo '{"otp_code": "..."}' | wb resume my-run --signal -   # stdin (agent-style)
```

See `examples/wait-demo.md` for an end-to-end example.

### Timeout reaping

Because `wb` is not a daemon, expired `wait` timeouts only fire when `wb`
next runs. `wb pending` handles this on every invocation: it sweeps
descriptors whose `timeout_at` has passed and whose `on_timeout` is `abort`
(or unset/unknown — both treated as abort on resume), marks the checkpoint
as failed, and deletes the pending descriptor. `skip` and `prompt` modes are
left alone because resolving them requires actually running the remaining
blocks, which `wb resume` does. Pass `--no-reap` for pure inspection.

## Artifacts

`wb` creates a per-run artifacts directory and exports `$WB_ARTIFACTS_DIR`
into every cell's env. Any cell (bash, python, browser) can drop files
there; later cells read them back. The browser runtime has a `save:` verb
that persists the previous `extract`/`eval` result into the dir.

Default path: `~/.wb/runs/<run_id>/artifacts/` when a run id is set via
`WB_RECORDING_RUN_ID` or `TRIGGER_RUN_ID`; otherwise a fresh tmp dir per run.

When `WB_ARTIFACTS_UPLOAD_URL` is set (template supports `{run_id}` and
`{filename}`), `wb` POSTs each new artifact to that URL after the cell that
wrote it completes, with `Authorization: Bearer $WB_RECORDING_UPLOAD_SECRET`.

See `examples/artifacts-demo.md`.

## Checkpointing

Resume workbook runs from where they stopped. Designed for agent workflows where blocks may fail due to external issues (API down, missing input, rate limits).

```bash
wb run deploy.md --bail --checkpoint deploy-1
# Block 3 fails — fix the issue (rotate API key, wait for service, etc.)
wb run deploy.md --bail --checkpoint deploy-1
# Resumes from block 3, skips already-completed blocks
```

- `--checkpoint <id>` saves progress after each block to `~/.wb/checkpoints/<id>.json`
- If a checkpoint exists for that ID (in_progress/failed), the run resumes from where it stopped
- With `--bail`, the failed block is re-run on resume (not skipped)
- Completed checkpoints start fresh on re-run (IDs are reusable)
- If the workbook file or block count changed, starts fresh

## Callbacks

HTTP POST notifications for step completions, checkpoint failures, and run completions. Designed for agent orchestration — an agent can listen for `checkpoint.failed` to know when human intervention is needed.

```bash
wb run deploy.md --bail --checkpoint deploy-1 \
  --callback https://hooks.example.com/wb \
  --callback-secret my-hmac-key
```

Three events:
- **`step.complete`** — after each block executes (pass or fail)
- **`checkpoint.failed`** — bail triggered on failure with checkpointing active
- **`run.complete`** — entire run finished

Headers sent:
- `Content-Type: application/json`
- `X-WB-Event: <event>`
- `X-WB-Signature: sha256=<hmac-sha256-hex>` (when `--callback-secret` is set)

Payloads include `checkpoint_id`, `workbook`, `progress`, and `timestamp`. The `checkpoint.failed` event includes `failed_block.stderr` for diagnostics.

## Sandbox execution

Workbooks can declare system/language deps in frontmatter and `wb` builds a Docker image, then re-invokes itself inside the container with the workbook mounted:

```yaml
---
title: PDF Pipeline
runtime: python
requires:
  sandbox: python          # python | node | custom
  apt: [qpdf, poppler-utils]
  pip: [pikepdf, pypdf]
---
```

When `requires:` is set, `wb`:

1. Generates a Dockerfile from the requires block (or uses `dockerfile:` for `sandbox: custom`).
2. Hashes the requires block into a deterministic image tag (`wb-sandbox:<12-char-hash>`) and reuses a cached image when nothing changed.
3. Mounts the workbook directory at `/work` and `~/.wb/checkpoints` at `/root/.wb/checkpoints` so checkpoint/pending state persists across container runs.
4. Forwards resolved secrets, env-file contents, frontmatter env, and CLI vars via `-e` flags.
5. Re-enters the container on `wb resume` for paused workbooks (pending descriptors live on the host via the mount).

```bash
wb run examples/sandbox-demo.md
wb containers list          # show cached sandbox images
wb containers build some/   # pre-build images for a folder of workbooks
wb containers prune         # remove all wb-sandbox images
wb inspect file.md          # shows resolved sandbox config + image tag
```

Docker must be installed and running. If missing or the image build fails, `wb` exits with code 5 (`EXIT_SANDBOX_UNAVAILABLE`). See `examples/sandbox-demo.md` for a minimal working example.

## Secret Providers

Configured in frontmatter or overridden via CLI flags:

- **env** — pull from environment variables
- **doppler** — `doppler secrets download`
- **yard** — `yard env get` (Paracord/OpenClaw)
- **command/cmd** — arbitrary shell command that outputs JSON or KEY=VALUE
- **dotenv/file** — read from .env file
- **prompt** — interactive terminal prompt

## Development

```bash
cargo build              # Debug build
cargo build --release    # Release build (~650KB)
cargo test               # Run tests

# Local dev alias
alias wb-dev='./target/release/wb'
```

## Design Principles

1. **Markdown is the format** — not .ipynb JSON blobs. Readable, diffable, agent-friendly.
2. **Zero runtime deps** — single binary, runs anywhere.
3. **Multi-runtime** — bash, python, node, ruby, whatever. Just spawn the right process.
4. **Secrets are pluggable** — doppler, yard, env, dotenv, prompt. Add more as needed.
5. **Output is markdown** — agents produce workbooks, agents consume results.
6. **Small and fast** — 650KB binary, <5ms startup.

## Install

```bash
curl -fsSL https://get.workbooks.dev | sh
```

## Website

The website repo is at `/Users/jmitch/Dev/workbooks-dev/workbooks.dev` (Cloudflare Workers).
