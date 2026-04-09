# Workbooks

Run markdown as code.

`wb` is a CLI that executes fenced code blocks in markdown files. Write docs that run. Share runbooks that actually work.

```
wb run deploy-check.md -o results.md
```

## Install

```bash
curl -fsSL https://get.workbooks.dev | sh
```

The installer drops a single ~650KB binary into `$HOME/.local/bin` — no sudo, no package manager, no runtime dependencies. If that directory isn't on your `PATH`, the installer will print the line to add to your shell profile.

### Install elsewhere

Override the install location with `WB_INSTALL_DIR`:

```bash
WB_INSTALL_DIR=/usr/local/bin curl -fsSL https://get.workbooks.dev | sh
```

The installer will error out if the target directory isn't writable by the current user — it never escalates with sudo.

### Build from source

```bash
git clone https://github.com/workbooks-dev/workbooks.git
cd workbooks
cargo build --release
# binary at ./target/release/wb
```

### Update

```bash
wb update           # download and replace the current binary
wb update --check   # check for a new release without installing
wb version          # print the installed version
```

`wb update` requires that the installed binary be writable by the current user. If it isn't, it'll point you at a reinstall command.

## Usage

```bash
wb run file.md                        # Run and show output
wb run file.md -o results.md          # Save results as markdown
wb run file.md --json                 # Emit JSON to stdout
wb run file.md --bail                 # Stop on first failure
wb run file.md --quiet                # Suppress block output in terminal
wb run file.md --secrets doppler      # Override secret provider
wb run file.md -C /path/to/dir        # Set working directory
wb run file.md --checkpoint my-run    # Save/resume execution state
wb run file.md --callback <url>       # POST events to a webhook
wb run folder/                        # Run every .md file in a folder
wb inspect file.md                    # Show structure without running
```

`wb` exits 0 on all-pass, 1 on any failure.

## Workbook Format

A workbook is a markdown file with optional YAML frontmatter. Code blocks with executable language tags are run in order. Non-executable fences (`yaml`, `json`, `toml`, `sql`, …) are preserved as documentation.

````markdown
---
title: My Workbook
runtime: python
venv: ./.venv
env:
  HOST: localhost
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
````

- **Frontmatter** configures runtime, venv, env vars, secrets, setup, exec prefixes, and working directory
- **Language tags** on each fence determine which runtime executes the block
- **Non-executable blocks** are treated as docs and skipped

A single workbook can mix languages. Each block runs in its own subprocess using whatever runtime matches the fence tag.

### Supported languages

`bash`, `sh`, `zsh`, `python`/`python3`/`py`, `node`/`javascript`/`js`, `ruby`/`rb`, `perl`, `php`, `lua`, `r`, `swift`, `go`

### Frontmatter reference

| Field | Type | Description |
|---|---|---|
| `title` | string | Workbook title (used in output) |
| `runtime` | string | Default runtime for untagged blocks |
| `env` | map | Environment variables injected into all blocks |
| `secrets` | object / array | Secret provider configuration |
| `setup` | string / array / object | Commands to run before blocks execute |
| `exec` | string / map | Execution prefix (e.g. `docker exec container`) |
| `venv` | string | Python virtualenv to activate |
| `working_dir` | string / map | Working directory (global or per-language) |
| `shell` | string | Shell override for bash blocks |

## Secret Providers

Configured in frontmatter or overridden via `--secrets`:

| Provider | Source |
|---|---|
| `env` | Environment variables |
| `doppler` | `doppler secrets download` |
| `yard` | `yard env get` (Paracord / OpenClaw) |
| `command` / `cmd` | Arbitrary shell command outputting JSON or `KEY=VALUE` |
| `dotenv` / `file` | `.env` file |
| `prompt` | Interactive terminal prompt |

Multiple providers can be merged in order:

```yaml
secrets:
  - provider: env
    keys: [API_KEY]
  - provider: dotenv
    command: .env.local
```

## Checkpointing

Resume workbook runs from where they stopped. Designed for agent workflows where blocks may fail due to external issues (API down, missing input, rate limits).

```bash
wb run deploy.md --bail --checkpoint deploy-1
# Block 3 fails — fix the issue (rotate API key, wait for service, etc.)
wb run deploy.md --bail --checkpoint deploy-1
# Resumes from block 3, skips already-completed blocks
```

- Progress saved to `~/.wb/checkpoints/<id>.json` after each block
- With `--bail`, the failed block is re-run on resume (not skipped)
- Completed checkpoints start fresh on re-run (IDs are reusable)
- If the workbook's block count changes, the checkpoint starts fresh

## Callbacks

HTTP POST notifications for step completions, checkpoint failures, and run completions. Designed for agent orchestration — a controller can listen for `checkpoint.failed` to know when human intervention is needed.

```bash
wb run deploy.md --bail --checkpoint deploy-1 \
  --callback https://hooks.example.com/wb \
  --callback-secret my-hmac-key
```

| Event | When |
|---|---|
| `step.complete` | After each block executes (pass or fail) |
| `checkpoint.failed` | Bail triggered on failure with checkpointing active |
| `run.complete` | Entire run finished |

Headers:

- `Content-Type: application/json`
- `X-WB-Event: <event name>`
- `X-WB-Signature: sha256=<hmac-sha256-hex>` (when `--callback-secret` is set)

Verify the signature with HMAC-SHA256 over the raw JSON body using the secret as the key.

## Claude Code Skill

This repo ships an agent skill at [`skills/workbooks/SKILL.md`](skills/workbooks/SKILL.md) that teaches Claude Code (and other Claude agents) how to author, run, and interpret workbooks on your behalf.

### Install the skill

Copy the skill directory into your Claude skills location:

```bash
# User-level (available in every project)
mkdir -p ~/.claude/skills
cp -r skills/workbooks ~/.claude/skills/workbooks

# Project-level (only in this repo or another project)
mkdir -p .claude/skills
cp -r skills/workbooks .claude/skills/workbooks
```

Once installed, Claude will pick up the skill automatically — no restart needed on the next session.

### What the skill does

With the skill loaded, Claude knows how to:

- Author workbooks with correct frontmatter, secret providers, exec prefixes, and setup commands
- Run workbooks with the right flags for the situation (`--bail`, `--checkpoint`, `--callback`, `--json`)
- Interpret JSON output and checkpoint state to recover from partial failures
- Use the checkpoint + callback pattern for agent-driven runs where external services are flaky

### Using the skill

Just ask in natural language. Claude will invoke the skill when the request matches:

- *"Write a deploy-check workbook for this service"*
- *"Run examples/health-check.md and tell me what failed"*
- *"Turn this bash script into a workbook with checkpointing"*
- *"Create a runbook for rotating the API key"*

## Examples

The [`examples/`](examples/) directory has workbooks you can run immediately:

- [`hello.md`](examples/hello.md) — basic multi-runtime execution
- [`health-check.md`](examples/health-check.md) — system health checks
- [`data-pipeline.md`](examples/data-pipeline.md) — data processing pipeline
- [`multi-runtime.md`](examples/multi-runtime.md) — bash, python, node, ruby in one file
- [`secrets-demo.md`](examples/secrets-demo.md) — secret provider usage (bash)
- [`secrets-python-demo.md`](examples/secrets-python-demo.md) — reading secrets from Python with `os.environ`, HMAC signing, fail-fast validation
- [`secrets-nodejs-demo.md`](examples/secrets-nodejs-demo.md) — reading secrets from Node with `process.env`, HMAC signing, fail-fast validation
- [`deploy-check.md`](examples/deploy-check.md) — deployment verification

```bash
wb run examples/hello.md
```

## Design

- **Markdown is the format** — not `.ipynb` JSON blobs. Readable, diffable, agent-friendly.
- **Zero runtime deps** — single Rust binary, ~650KB, <5ms startup.
- **Multi-runtime** — bash, python, node, ruby, whatever. Just spawn the right process.
- **Secrets are pluggable** — doppler, yard, env, dotenv, prompt. Add more as needed.
- **Output is markdown** — agents produce workbooks, agents consume results.
- **No sudo, ever** — installs to your home dir, updates in place, runs anywhere.

## Development

```bash
cargo build              # Debug build
cargo build --release    # Release build (~650KB)
cargo test               # Run tests
```

## License

MIT
