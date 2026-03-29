---
name: workbooks
description: Run markdown files as executable notebooks using the `wb` CLI. Use this skill when the user wants to create, run, inspect, or debug workbooks — markdown files with fenced code blocks that execute as multi-runtime scripts. Also use when the user needs runbooks, health checks, deploy verification, data pipelines, or any "executable documentation" workflow.
metadata:
  openclaw:
    requires:
      bins:
        - wb
---

# Workbooks (`wb`)

Run markdown as code. Write docs that run. Share runbooks that actually work.

## What It Does

`wb` executes fenced code blocks in markdown files. Each code block runs in its own subprocess using the language specified in the fence tag. Blocks run sequentially and share state within the same runtime (e.g., bash blocks share the same shell session).

## Commands

```bash
wb run file.md                   # Run workbook, show summary
wb run file.md --json            # Run and output JSON results to stdout
wb run file.md -o results.json   # Run and save results to file (format from extension)
wb run file.md -o results.md     # Save as annotated markdown with results
wb run file.md --bail            # Stop on first failure
wb run file.md --verbose         # Show block output in terminal
wb run file.md -C /path/to/dir   # Set working directory
wb run file.md --checkpoint id   # Save/resume execution with checkpoint ID
wb run file.md --callback <url>  # POST events to webhook URL
wb run file.md --callback-secret key  # HMAC-SHA256 signing for callbacks
wb run folder/                   # Run all .md files in a folder
wb inspect file.md               # Show structure without running
```

## Workbook Format

A workbook is a markdown file with optional YAML frontmatter. Code blocks with executable language tags are run in order. Non-executable blocks (yaml, json, toml, etc.) are preserved as documentation.

### Minimal workbook

````markdown
# My Workbook

```bash
echo "hello from bash"
```

```python
print("hello from python")
```
````

### With frontmatter

````markdown
---
title: Deploy Check
runtime: bash
env:
  HOST: localhost
  PORT: "8080"
secrets:
  provider: env
  keys: [API_KEY, DB_URL]
setup: npm install
exec:
  python: uv run
working_dir: ./src
---

# Deploy Check

```bash
curl -s http://${HOST}:${PORT}/health
```
````

## Frontmatter Reference

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Workbook title (used in output) |
| `runtime` | string | Default runtime for untagged blocks |
| `env` | map | Environment variables injected into all blocks |
| `secrets` | object/array | Secret provider configuration (see below) |
| `setup` | string/array/object | Commands to run before blocks execute |
| `exec` | string/map | Execution prefix (e.g., `docker exec container`) |
| `venv` | string | Python virtualenv path to activate |
| `working_dir` | string/map | Working directory (global or per-language) |
| `shell` | string | Shell override for bash blocks |

### Secret providers

```yaml
# Single provider
secrets:
  provider: env
  keys: [API_KEY, DB_URL]

# Multiple providers (merged in order)
secrets:
  - provider: env
    keys: [API_KEY]
  - provider: dotenv
    command: .env.local

# Available providers: env, doppler, yard, dotenv, command, prompt
```

### Exec prefixes (remote/containerized execution)

```yaml
# All blocks run through docker
exec: "docker exec mycontainer"

# Per-language
exec:
  python: "uv run"
  node: "pnpm exec"
```

### Setup commands

```yaml
# Single command
setup: uv sync

# Multiple commands
setup:
  - uv sync
  - npm install

# With working directory
setup:
  dir: ../../
  run:
    - uv sync
    - npm install
```

## Supported Languages

bash, sh, zsh, python/python3/py, node/javascript/js, ruby/rb, perl, php, lua, r, swift, go

Non-executable fences (yaml, json, toml, sql, etc.) are treated as documentation and skipped during execution.

## Output Formats

### JSON (`--json` or `-o results.json`)

```json
{
  "source": "deploy-check.md",
  "title": "Deploy Check",
  "ran_at": "2025-01-15T10:30:00Z",
  "duration_ms": 1250,
  "status": "pass",
  "blocks": { "total": 3, "passed": 3, "failed": 0 },
  "results": [
    {
      "index": 0,
      "language": "bash",
      "status": "pass",
      "exit_code": 0,
      "duration_ms": 450,
      "stdout": "HTTP/1.1 200 OK",
      "stderr": ""
    }
  ]
}
```

### Markdown (`--md` or `-o results.md`)

Produces an annotated copy of the original workbook with results inlined after each code block.

## Checkpointing

Resume workbook runs from where they stopped. Use `--checkpoint <id>` to save execution progress and resume on retry. This is essential for agent workflows where external failures (API down, missing credentials, rate limits) may interrupt a run.

```bash
# First run — block 3 fails because API is down
wb run deploy.md --bail --checkpoint deploy-1

# Fix the issue (wait for API, rotate key, etc.), then resume
wb run deploy.md --bail --checkpoint deploy-1
# Skips blocks 1-2, re-runs block 3
```

### Behavior

- Progress saved to `~/.wb/checkpoints/<id>.json` after each block
- Existing checkpoint with matching workbook/block count → resume from where it stopped
- With `--bail`: the failed block is re-run on resume (not skipped)
- Without `--bail`: all blocks run; checkpoint only helps if process was killed mid-run
- Completed checkpoint → starts fresh (IDs are reusable)
- Workbook changed (different block count) → starts fresh

### When to use checkpoints

- **Long-running workbooks** with many sequential blocks
- **Deploy workflows** where external services may be flaky
- **Agent-driven runs** where blockers need human intervention before retry
- Pair with `--bail` for the "fix and retry" pattern

## Callbacks

HTTP POST notifications for agent orchestration. Use `--callback <url>` to receive events as blocks execute.

```bash
wb run deploy.md --bail --checkpoint deploy-1 \
  --callback https://hooks.example.com/wb \
  --callback-secret my-hmac-key
```

### Events

| Event | When | Key payload fields |
|-------|------|--------------------|
| `step.complete` | After each block executes | `block.index`, `block.exit_code`, `progress.completed` |
| `checkpoint.failed` | Bail triggered with checkpoint active | `failed_block.index`, `failed_block.stderr`, `checkpoint_id` |
| `run.complete` | Entire run finished | `status` (pass/fail), `blocks.passed`, `blocks.failed`, `duration_ms` |

### Headers

- `Content-Type: application/json`
- `X-WB-Event: <event name>`
- `X-WB-Signature: sha256=<hmac-sha256-hex>` (when `--callback-secret` provided)

### Signature verification

The signature is HMAC-SHA256 of the raw JSON body using the secret as key. Verify in your webhook handler:

```python
import hmac, hashlib
expected = hmac.new(secret.encode(), body, hashlib.sha256).hexdigest()
assert signature == f"sha256={expected}"
```

### When to use callbacks

- **Agent orchestration** — notify a controller when a step fails so it can intervene
- **CI/CD pipelines** — post results to Slack, PagerDuty, or a dashboard
- **Monitoring** — track workbook execution across a fleet of machines

## Authoring Workbooks

When creating workbooks for the user:

1. **Use frontmatter** for configuration that applies to the whole workbook (env vars, secrets, working directory)
2. **Use headings** to organize blocks into logical sections with descriptions
3. **Use non-executable fences** (yaml, json) for documentation/examples that shouldn't run
4. **Use `--bail`** for sequential checks where later blocks depend on earlier ones
5. **Keep blocks focused** — one concern per block, with a heading explaining what it does
6. **Use exit codes** for pass/fail — `exit 1` in bash or `sys.exit(1)` in python signals failure

### Example: Health check workbook

````markdown
---
title: Service Health Check
env:
  HOST: "${DEPLOY_HOST:-localhost}"
---

# Service Health Check

## HTTP endpoint

```bash
STATUS=$(curl -s -o /dev/null -w "%{http_code}" "http://${HOST}/health")
if [ "$STATUS" != "200" ]; then
  echo "FAIL: HTTP $STATUS" >&2
  exit 1
fi
echo "OK: HTTP $STATUS"
```

## Database connectivity

```bash
pg_isready -h ${HOST} -p 5432 -q && echo "OK: postgres" || exit 1
```

## Response time

```bash
TIME=$(curl -s -o /dev/null -w "%{time_total}" "http://${HOST}/")
echo "Response: ${TIME}s"
if [ "$(echo "$TIME > 2" | bc)" = "1" ]; then
  echo "SLOW: ${TIME}s > 2s threshold" >&2
  exit 1
fi
```
````

## Running and Interpreting Results

When running workbooks on behalf of the user:

1. Use `--json` when you need to parse results programmatically
2. Use `--verbose` when the user wants to see block output in real time
3. Use `--bail` for workbooks where blocks are dependent (deploy checks, setup scripts)
4. Use `--checkpoint <id>` with `--bail` for workflows that may hit external blockers — this lets you retry from the failed block after fixing the issue
5. Use `--callback <url>` to get notified of step completions and failures — combine with `--checkpoint` for full agent orchestration
6. Check the exit code: `wb` exits 0 on all-pass, 1 on any failure
7. For folder runs, `wb run folder/ --json` returns a batch report with per-workbook summaries

## Install

```bash
curl -fsSL https://get.workbooks.dev | sh
```
