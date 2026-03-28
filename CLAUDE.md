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
wb run file.md -v                     # Show block output in terminal
wb run file.md --secrets doppler      # Override secret provider
wb run file.md -C /path/to/dir        # Set working directory
wb inspect file.md                    # Show structure without running
```

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
