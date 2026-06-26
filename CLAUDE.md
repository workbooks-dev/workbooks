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
│   ├── mcp.rs         # `wb mcp` — Model Context Protocol server over stdio
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

**wb does not impose a default block timeout.** A block runs until it exits,
the parent process dies, or the user signals. Wall-clock caps are opt-in —
authorial intent wins, since the author of a long DDL or batch step knows
it's long-running. Three places to set one (highest precedence first):

1. **Per-block (fence attr or frontmatter map)** — the cap stays attached to
   the block.
2. **`timeouts._default` in frontmatter** — runbook-wide safety net applied to
   every block that doesn't have its own override.
3. **`--default-block-timeout <dur>` CLI flag** — convenient for CI hosts or
   scheduled agents that want to enforce a cap without editing the runbook.

If none of these are set, blocks run unbounded.

Two equivalent ways to set the per-block policy. The fence-attr form is
preferred — the policy stays attached to the block across edits, so inserting
a block above doesn't shift every downstream entry.

**Fence attrs** (Pandoc-style `{key=value}` cluster):

```markdown
```bash {#health timeout=30s retries=2}
curl -sf https://example.com
```

```bash {#cleanup continue_on_error}
rm -rf $TMPDIR
```
```

**Frontmatter maps** (legacy, keyed by 1-based block number — plus the special
`_default` key for a runbook-wide cap):

```yaml
---
timeouts:
  _default: 30m       # runbook-wide safety net (optional)
  1: 30s              # tighter cap on block 1
  3: 2m
retries:
  3: 2                # retry block 3 up to 2 more times on failure
continue_on_error: [4] # block 4 failure doesn't trigger --bail
---
```

- **`timeouts`** — values are duration strings (`30s`, `5m`, `2h`, bare int = seconds). A timed-out block gets `error_type: "timeout"` and `stdout_partial: true` / `stderr_partial: true` in JSON output and callback payloads — partial output is preserved so agents can diagnose hung blocks. A timeout kills the language session child; a later retry or block will spawn a fresh session (state reset). When a timeout fires, wb prints which knob set the cap (`fence attr`, `frontmatter timeouts.<N>`, `frontmatter _default`, or `--default-block-timeout`) so the operator knows where to extend it.
- **`retries`** — number of *additional* attempts after the first failure (`0`/missing = no retry). Retries run with a 500ms delay between attempts. Useful for flaky HTTP calls; combine with `timeouts:` to cap individual attempts.
- **`continue_on_error`** — block numbers whose failure should not halt a `--bail` run. The block's failure is still recorded and emitted via callbacks; execution just continues to the next block.

When a block has both a fence attr and a legacy frontmatter entry for the same field, **the fence attr wins** and `wb validate` emits a `wb-step-002` warning so you know to drop the legacy entry.

The "no default cap" rule applies to process-runtime blocks (bash, python,
node, ruby, sandbox). Browser slices, `wait` blocks, and other sidecar verbs
already have their own protocol-specific timeouts and are unaffected.

Callback payloads (`step.complete`, `checkpoint.failed`) include `stdout_partial` / `stderr_partial` fields so downstream agents can distinguish "block failed" from "block was cut off mid-run".

## Stable step IDs

Every executable block gets a stable identifier that flows into callback
payloads (`block.step_id`), `wb inspect --json` (`blocks[].step_id`), and
future selective-run flags. Two ways to set it:

- **Explicit**: `{#login}` — Pandoc-style id attribute. Survives edits.
- **Auto-derived**: a deterministic `auto-<12-hex-chars>` hash of the include
  chain + position + language + first 64 bytes of the body. Same workbook
  produces the same auto ids on every parse.

Duplicate explicit ids are a `wb validate` error (`wb-step-001`). Auto ids
won't collide in practice since position is part of the hash.

See `examples/step-ids-demo.md`.

### Selective runs: `--only`, `--from`, `--until`, `--tag`

Step ids (and fence `.class` tags) are the substrate for picking a subset of a
workbook to run:

```bash
wb run deploy.md --only login              # just run the login block
wb run deploy.md --from migrate            # start at migrate, run to end
wb run deploy.md --until smoke-test        # stop after smoke-test
wb run deploy.md --from migrate --until smoke-test   # bounded range
wb run deploy.md --tag smoke               # only blocks tagged {.smoke}
wb run deploy.md --tag smoke --tag db      # union of .smoke and .db blocks
```

`--only`/`--from`/`--until` take a step id — either explicit (`{#login}`) or
auto-derived (`auto-<hash>`). `--tag` takes a fence `.class` (repeatable; a
block matches if it carries any of the given classes) and composes with
`--from`/`--until` as an intersection. Unknown step ids, and tags that match no
block, fail with a usage error before any block runs. Skipped blocks emit
`step.skipped` callbacks with `kind: "selection"` so agents see the gap.

Limits in this milestone:

- `--only` conflicts with `--from`/`--until`/`--tag` (clap rejects at parse).
- Selection cannot be combined with `--checkpoint` — partial-run state
  semantics aren't defined yet (which "completed" do we track when most
  blocks are intentionally skipped?). Run ephemerally instead.
- A selective run is *ephemeral*: it doesn't read or write the default
  checkpoint, so subsequent normal runs still see the previous state.
- `--changed` and the source-hash cache (`--no-cache`) are tracked in #33.

### Source-hash execution cache: `--cache`

`wb run <file> --cache <id>` enables a per-id cache at `~/.wb/cache/<id>.json`.
A block is **skipped** on re-run when its source + parameter identity is
byte-identical to a previously *successful* run under that id — the "skip
unchanged blocks" memoization that makes iterative agent re-runs fast.

```bash
wb run pipeline.md --cache pipe       # first run executes + records
wb run pipeline.md --cache pipe       # unchanged blocks are skipped
wb run pipeline.md --cache pipe --no-cache   # force a full run
```

- **Cache key** = sha256(language + body + param hash). Editing a block, or
  changing `--param`/`--profile`, invalidates just that block's entry.
- A cached block is *skipped*, not replayed — its stdout/outputs are not
  reproduced. Use the cache for **idempotent** pipelines.
- A side-effecting block opts out with the `{no-cache}` fence flag so it always
  runs. `--no-cache` disables caching for the whole run.
- Skips emit `step.skipped` with `kind: "cache"`.
- Not yet in the key (tracked under #18/#33): env/secret identity,
  included-file hashes, runtime versions. Change the cache id when those change.

### Dry-run preview: `--dry-run`

`wb run <file> --dry-run` resolves params, selection, conditionals, and per-step
policy, then prints the execution plan — each block marked `run`/`skip` with the
reason (selection, `no-run`, `when=`/`skip_if=`) and the resolved command — and
exits without running anything. It does **not** resolve secrets or run setup, so
conditionals are evaluated against frontmatter env + vars + params only.

```bash
wb run deploy.md --dry-run --profile prod
```

## Conditional cells: `{when=…}` and `{skip_if=…}`

Runtime-conditional execution via info-string attributes — same attribute cluster
as `{no-run}` and `{silent}`:

```markdown
```bash {when=$DEPLOY_ENV=prod}
deploy --to prod
```

```bash {skip_if=$DRY_RUN}
rm -rf $WORKDIR
```

```python {when=$FEATURE_X, silent}
run_experiment()
```
```

**Expression grammar** (intentionally tiny, no shell, no arithmetic):

- `$VAR` — truthy: non-empty, and not `0`/`false`/`no`/`off` (case-insensitive)
- `$VAR=value` — env[VAR] equals `value`
- `$VAR!=value` — env[VAR] does not equal `value` (a missing var is "not equal" to anything)
- `!<expr>` — boolean NOT of any of the above

Values can't contain spaces — the info-string tokenizer splits on whitespace. A
block runs when `when` is truthy *and* `skip_if` is falsy (AND composition).

**Eval env** — process env merged with the workbook's session env (frontmatter +
resolved secrets + `--env` CLI + `WB_*` internals), session values win on conflict.
This matches what a bash block actually sees at runtime, so `skip_if=$CI` behaves
as expected when `CI=1` is set in the parent shell.

**Gating on a prior step's output** — a captured output (see "Structured step
outputs" below) is exported into the eval env under a `WB_OUT_` prefix, so a
later cell can branch on a value an earlier step computed. A step that prints
`output: needs_login=1` makes `$WB_OUT_needs_login` available to every
subsequent block's `{when=...}` / `{skip_if=...}` evaluation. This is how you
make a pause conditional: an earlier slice evals login state and emits
`output: needs_login=...`, then a later `browser {when=$WB_OUT_needs_login}`
holds the `pause_for_human` — a warm/already-authenticated run skips the pause
and runs straight through; a cold run stops. (Per-*verb* conditionals inside a
single slice are not supported — gate at the fence level on a separate slice.)

Scope note: the export feeds the `{when=}` / `{skip_if=}` evaluator (which reads
the session env directly). It is not re-injected into already-running persistent
shell sessions, so a bash block that ran before the output was produced won't see
`$WB_OUT_*` in its own process env — read the value back from `$WB_OUTPUTS_PATH`
if a cell needs it at runtime.

**Skip semantics** — same as `{no-run}`: no execution, no callback, no checkpoint,
`block_idx` does not advance. Unlike `{no-run}`, a conditionally-skipped block
still counts toward `blocks.total` (can't be filtered at parse time), so callback
streams show a gap (e.g. events for 1, 2, 4, 5 out of 5 blocks). Malformed
expressions log a warning and skip the block fail-safe.

See `examples/conditional-demo.md` for a runnable example, and
`examples/conditional-pause-demo.md` for gating a step on a prior step's output.

## Typed parameters and profiles

Declare parameters in frontmatter and supply values at run time. Each param has
an optional `type` (`string` default | `int` | `bool` | `enum`), `default`,
`required` flag, `one_of` choices, and `secret` flag. A bare scalar is shorthand
for a defaulted string param.

```yaml
---
params:
  region:
    type: enum
    one_of: [us-east-1, eu-west-1]
    default: us-east-1
  replicas:
    type: int
    default: 2
  dry_run:
    type: bool
    default: true
  service: api            # shorthand: scalar = default, type string
profiles:
  prod:
    region: eu-west-1
    replicas: 6
    dry_run: false
---
```

```bash
wb run deploy.md --param replicas=10        # override one value
wb run deploy.md --profile prod             # apply a named preset
wb run deploy.md --param-file values.yaml   # YAML mapping of name: value
```

- **Precedence** (highest first): `--param` > `--param-file` > selected
  `--profile` > declared `default`.
- **Validation at run start** (before any block): values are checked against
  `type` and `one_of`; an undeclared `--param`/`--param-file` key, a missing
  `required:` param, or a bad value is a usage error (exit 2). `wb validate`
  statically checks the declarations (`wb-param-001`) and profiles
  (`wb-param-002`).
- **Injection**: resolved values are exported into every cell's env under their
  bare name (`$region`, `$replicas`, …) and are visible to `{when=}` /
  `{skip_if=}`. `secret: true` values are redacted from rendered output.
- **Checkpoint identity**: the resolved set is hashed into the checkpoint
  (`param_hash`). Re-running a checkpoint with different params starts fresh
  instead of mixing state; the resolved values are persisted so `wb resume`
  re-applies them (resume carries no `--param` flags, and a `required:` param
  has no default to fall back on).
- Cache identity (#18) and include-level param passing are not yet wired.

See `examples/params-demo.md`.

## Inline assertions and `wb test`

Follow an executable block with an `expect` (or `assert`) fence to assert on its
result. `wb test` runs the workbook and evaluates the assertions with CI-friendly
exit codes; a plain `wb run` does not evaluate them.

```markdown
```bash
curl -sf https://example.com/health
```

```expect
exit 0
stdout contains "ok"
stderr empty
```
```

**Grammar** — one assertion per line (`#` comments and blanks ignored), checked
against the immediately preceding executable block:

- `exit <N>` / `exit-code <N>` — exit code equals N; `exit != <N>` for inequality
- `stdout contains <text>` / `stderr contains <text>` — substring present
- `stdout not-contains <text>` — substring absent
- `stdout equals <text>` — exact match (trimmed)
- `stdout empty` / `stdout not-empty`

`<text>` may be quoted (`"…"` / `'…'`) to include spaces. The DSL is tiny and
dependency-free (no regex, no shell). `wb validate` reports malformed lines as
`wb-expect-001`.

```bash
wb test deploy.md                 # human report
wb test deploy.md --format json   # machine-readable {ok, passed, failed, files[]}
wb test ./runbooks                # every *.md in the folder
```

Exit codes: `0` all assertions pass, `1` any assertion fails or a file errors,
`2` no `expect`/`assert` fences found or a usage error. Artifact/file assertions,
browser selector assertions, and JUnit/GitHub-annotation output are deferred.

See `examples/test-demo.md`.

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

### Declarative prerequisites: `required:`

Sugar over `include:` for "this runbook needs A and B to run first." Each entry
in the `required:` frontmatter list is prepended at position 0 as a synthetic
include — same execution path, same cycle/missing-file errors, same
`IncludeEnter`/`IncludeExit` sentinels — but expressed as configuration rather
than an inline fence:

```yaml
---
required:
  - ./login.md
  - ./warm-cache.md
---
```

Order in the list = execution order. Notes:

- *Not recursive*: an included workbook's own `required:` is ignored (its
  frontmatter is ignored entirely, matching the include contract). Treat this
  like a flat "needs:" list, not transitive deps.
- An empty list is a no-op.
- Distinct from the existing `requires:` field (Docker sandbox config) — note
  the trailing `d`.

See `examples/required-demo.md` for a runnable example.

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
wb run file.md --only <step-id>       # Run only this step; skip the rest
wb run file.md --from <step-id>       # Start at this step (skip earlier)
wb run file.md --until <step-id>      # Stop after this step (inclusive)
wb run file.md --default-block-timeout 30m  # Opt-in default cap for every block
wb run file.md --param region=us-east-1     # Set a declared typed parameter
wb run file.md --profile prod               # Apply a named parameter profile
wb run file.md --param-file values.yaml     # Load params from a YAML mapping
wb run file.md --tag smoke                  # Run only blocks with the .smoke fence class
wb run file.md --dry-run                    # Print the execution plan without running
wb run file.md --cache <id>                 # Skip unchanged blocks (source-hash cache)
wb test file.md                       # Run + evaluate expect/assert fences (CI exit codes)
wb test some/ --format json           # Test every *.md in a folder, machine-readable
wb artifacts list --run <id>          # List a run's captured artifacts (manifest)
wb artifacts open <name> --run <id>   # Print an artifact's absolute path
wb artifacts export <name> --to <dst> # Copy an artifact out of the run dir
wb runs list                          # List known runs (newest first)
wb runs show <id>                     # Show a run's artifacts + checkpoint state
wb inspect file.md                    # Show structure without running
wb pending                            # List paused workbooks (auto-reaps expired abort-mode descriptors)
wb pending --no-reap                  # List without reaping — safe for automation/inspection
wb resume <id> --signal <file>        # Resume a paused workbook with a signal payload
wb resume <id> --rerun-step [step]    # Re-run the current (or named) step instead of resuming forward
wb resume <id> --goto-step <step>     # Jump the cursor to a step (re-runs earlier / skips later)
wb cancel <id>                        # Drop a paused workbook without resuming
wb validate file.md                   # Static analysis (no execution); --format json, --strict
wb doctor                             # Environment health checks; --deep for Docker/sidecar/Redis probes
wb config set callback.url <url>      # Persist machine-wide defaults in ~/.wb/config.yaml
wb config list                        # Show set values + known keys (also: get/unset/path)
wb completion <shell>                 # Print a shell completion script (bash, zsh, fish, …)
wb man                                # Print a roff man page to stdout
wb mcp                                # Run a Model Context Protocol server over stdio (for agents)
wb version --format json              # Management commands take --format text|json
wb --log-level error <cmd>            # Global stderr verbosity (error|warn|info|debug)
```

## Stderr verbosity: `--log-level`

A global `--log-level <error|warn|info|debug>` (also `$WB_LOG_LEVEL`, default
`info`) gates warning/diagnostic stderr output. Lowering it to `error` silences
the noisy checkpoint/outputs/upload/callback warnings — useful for CI and agents
that want clean stderr. The level is process-global and works before or after the
subcommand. Essential run feedback (progress, results, the run summary) is never
suppressed. Implemented in `src/logging.rs` as gate macros (`log_warn!` etc.) over
`eprintln!` — zero new dependencies.

## Consistent JSON for management commands

`version`, `config` (get/set/unset/list/path), `containers` (list/build/prune),
and `cancel` all accept `--format json` (alongside the existing `validate` /
`doctor` / `pending` / `inspect`), so agents can script every management command
uniformly. JSON goes to stdout (pretty-printed); human messages and errors stay on
stderr; exit codes are unchanged (`config get` on an unset key still exits 2 but
emits `{"key":…,"value":null}` first).

## Machine-wide config: `wb config`

`wb config` manages a small allowlisted key/value store at `~/.wb/config.yaml`
(override with `$WB_CONFIG_PATH`). It's the "set my dashboard webhook once" layer.
Keys are validated against a known set on `set`, so a typo is rejected rather than
silently stored. Subcommands: `get <key>`, `set <key> <value>`, `unset <key>`,
`list`, `path`.

Known keys today are the callback defaults — `callback.url`, `callback.secret`,
`callback.key`. They're consulted at run start as the **lowest-precedence**
fallback: `--callback*` flag > `WB_CALLBACK_*` env var > config file. A malformed
config file warns and is ignored rather than aborting the run.

Callback URLs are validated up front (`http`/`https`/`redis`/`rediss` only), so a
bad endpoint fails fast with one clear message instead of a per-event curl error;
a Redis URL paired with an HMAC secret, or a plaintext `http://` endpoint, warns.

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

### Operator navigation at a browser pause

By default `wb resume` continues the paused browser slice forward (at
`verb_index + 1`). At a `pause_for_human`, an operator can instead pick a
different cell to run next — without restarting and losing the (expensive)
browser session:

```bash
wb resume <id> --rerun-step            # re-run the currently paused step from verb 0
wb resume <id> --rerun-step <step-id>  # re-run starting at an earlier step
wb resume <id> --goto-step <step-id>   # jump the cursor to step-id
```

- `--rerun-step` (no value) is the "run now" button: log in manually in the
  live browser, then re-run the verify step instead of bailing.
- `--goto-step <earlier-id>` re-runs the intervening steps; `--goto-step
  <later-id>` skips them, emitting `step.skipped` (kind `goto`) for each so the
  run-page timeline stays honest.
- A rerun/goto runs the target slice **fresh from its first verb** (the paused
  slice's sidecar state is not restored). Re-running a side-effecting cell
  re-applies its side effects — same as `wb run --from`.
- `--rerun-step` and `--goto-step` are mutually exclusive. Targets are stable
  `step_id`s; an unknown id is a usage error before anything runs.
- Run pages deliver the same choice through the resume signal payload:
  `{"action": {"kind": "goto_step", "target": "open-inbox"}}` (CLI flags win
  over the signal). Action targets declared on a `pause_for_human`'s `actions:`
  are validated at pause time, so the page never shows a dead button.

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

### Artifact manifest + `wb artifacts` / `wb runs`

Every run writes a `manifest.json` into its artifacts dir recording each
artifact with its size, content type, **sha256 checksum**, label/description
(from the `.meta.json` sidecar), the **step id that produced it**, and an
`updated_at` timestamp. Inspect captured artifacts after a run:

```bash
wb runs list                          # known runs, newest first, with counts
wb runs show <run-id>                 # a run's artifacts + checkpoint state
wb artifacts list --run <run-id>      # the run's manifest (--format json too)
wb artifacts open report.csv --run <run-id>      # prints the absolute path
wb artifacts export report.csv --to ./out.csv --run <run-id>
```

Runs live under `~/.wb/runs/<run-id>/artifacts` (a run id comes from
`WB_RECORDING_RUN_ID` / `TRIGGER_RUN_ID`). Omitting `--run` targets the most
recent run. For runs without a persisted manifest (older runs or external
tooling), the commands fall back to scanning the directory (no step
provenance). JSON output is available on `list`/`runs list`/`runs show` via
`--format json`.

### Browser-runtime auto-capture

Anything the `browser` runtime downloads during a session — clicked
attachments, redirect chains that end in a file, popup downloads — is
saved to `$WB_ARTIFACTS_DIR` automatically by the sidecar's context-level
listener. No `download:` verb to call. Provenance (source URL, page URL,
which verb was running) rides along on the `slice.artifact_saved` frame
so the run-page event feed shows *why* a file appeared.

Filter with `WB_BROWSER_DOWNLOAD_EXTENSIONS` (comma-separated, e.g.
`pdf,xlsx,csv`). Unset = capture everything. Skipped downloads still
emit `slice.download_skipped` for visibility.

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

## MCP server (`wb mcp`)

`wb mcp` exposes wb to the agent ecosystem as a **Model Context Protocol** server
over stdio. An MCP client (Claude, an inspector, an orchestrator) can author a
workbook, run it, get paused for human input, resume it, and read back results —
all over JSON-RPC.

```bash
wb mcp   # speaks JSON-RPC 2.0 over newline-delimited stdin/stdout; logs to stderr
```

It is a **thin adapter, not a daemon**. The core stays a CLI: `wb mcp` shells out
to the same `wb` binary (`current_exe`) for `run`/`inspect`/`validate`/`resume`/
`pending`, and reads checkpoint + pending state in-process (read-only) for
`get_run_events`. This is deliberate — `run_single` diverges via `process::exit`
on pause (code 42) and completion, so a subprocess boundary is what turns that
exit code into a value the server can map without the run engine killing the
long-lived server. Zero new dependencies (`serde_json` only). Implemented in
`src/mcp.rs`.

**Tools** (`tools/list`):

- `author_workbook {path, content, overwrite?}` — write a `.md` workbook to disk.
- `run_workbook {file, run_id?, vars?, dir?, bail?}` — execute. Returns `run_id`
  (= checkpoint id) and a task `status`. `bail` defaults true.
- `resume_workbook {run_id, value? | signal? | action?, rerun_step?, goto_step?}` —
  satisfy a paused run. `value` is the single-bind shortcut; `signal` is a full
  JSON payload; `action`/`rerun_step`/`goto_step` map to the resume navigation
  flags.
- `inspect_workbook {file}` / `validate_workbook {file, strict?}` — structure /
  static analysis as JSON (no execution).
- `list_pending {}` — runs awaiting input (read-only; does not reap timeouts).
- `get_run_events {run_id}` — replay a run's step timeline (`step.complete` /
  `step.skipped` + a terminal event) reconstructed from the durable checkpoint.

**State mapping (durable execution → MCP primitives):**

- **Checkpoint + pending descriptor = the Task store.** A run is keyed by a
  `run_id` that is also its checkpoint id; `list_pending` is the set of tasks
  awaiting input; `get_run_events` is a task's timeline.
- **`pause_for_human` / `wait` → elicitation.** A paused run returns
  `status: "input_required"` plus an `elicitation` object (message + a
  `requestedSchema` with one property per bound var). The client collects the
  input and calls `resume_workbook`. We surface this as data-in-the-result
  rather than a server-initiated `elicitation/create` round-trip because the
  producing subprocess has already exited — there is nothing to hold open.
- **Task status rides on the child exit code:** 0 → `completed`,
  42 → `input_required`, 1 → `failed`, 7 → `timeout`, others → an error category.

The full author→run→pause→resume→read lifecycle is covered end-to-end by
`tests/mcp_e2e.rs`, which drives the JSON-RPC server exactly as a client would.

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
