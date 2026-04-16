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
wb pending                            # List paused workbooks (experimental)
wb resume <id> --signal <file>        # Resume a paused workbook with a signal payload
wb cancel <id>                        # Drop a paused workbook without resuming
```

## Pausing on external signals (experimental)

Behind `WB_EXPERIMENTAL_WAIT=1`, workbooks can pause on a `wait` fence until an
external resolver (an agent, webhook handler, cron job, or a human) delivers the
awaited payload:

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

## Sandbox execution (experimental)

Behind `WB_EXPERIMENTAL_SANDBOX=1`, workbooks can declare system/language deps in frontmatter and `wb` builds a Docker image, then re-invokes itself inside the container with the workbook mounted:

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
WB_EXPERIMENTAL_SANDBOX=1 wb run examples/sandbox-demo.md
wb containers list          # show cached sandbox images
wb containers build some/   # pre-build images for a folder of workbooks
wb containers prune         # remove all wb-sandbox images
wb inspect file.md          # shows resolved sandbox config + image tag
```

Without the flag, a workbook with `requires:` exits 1 with an error asking you to set `WB_EXPERIMENTAL_SANDBOX=1`. See `examples/sandbox-demo.md` for a minimal working example.

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
