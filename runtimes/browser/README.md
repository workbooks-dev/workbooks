# wb-browser-runtime

Browser sidecar for `wb` — deterministic Playwright slices over Browserbase.

Each `browser` fenced block in a workbook arrives as one `slice` message;
this sidecar dispatches its `verbs` against a `playwright-core` `Page`
connected to a Browserbase session via CDP. Sessions are cached by `session:`
name across slices for the lifetime of the sidecar process so a runbook with
multiple browser blocks against the same vendor reuses one logged-in browser
context.

## Install (local dev)

```bash
cd runtimes/browser
npm install       # installs playwright-core
npm link          # exposes `wb-browser-runtime` on $PATH
```

Or set `WB_BROWSER_RUNTIME=/absolute/path/to/bin/wb-browser-runtime.js` for a
specific run.

## Required env

- `BROWSERBASE_API_KEY`
- `BROWSERBASE_PROJECT_ID`

Verb arguments support `{{ env.NAME }}` substitution at dispatch time, so any
secrets your runbook needs (e.g. `HACKERNEWS_PASSWORD`) get pulled from the
sidecar process env without ever appearing on stdout.

## Usage

```bash
WB_EXPERIMENTAL_BROWSER=1 wb run examples/browser-demo.md
```

See `examples/browser-demo.md` for a minimal workbook that exercises the
protocol against the Playwright-pause demo. For a real Browserbase end-to-end
example, see the `browserbase-hn-upvoted-probe` runbook in the xatabase repo.

## Verbs

| Verb         | Bare arg form               | Object form fields                              |
|--------------|-----------------------------|-------------------------------------------------|
| `goto`       | `goto: <url>`               | `url`, `wait_until`, `timeout`                  |
| `fill`       | —                           | `selector`, `value`, `timeout`                  |
| `click`      | `click: <selector>`         | `selector`, `timeout`                           |
| `press`      | `press: <key>`              | `key`, `selector`, `timeout`                    |
| `wait_for`   | `wait_for: <selector>`      | `selector`, `state`, `timeout`                  |
| `screenshot` | `screenshot: <path>`        | `path`, `full_page`                             |
| `extract`    | —                           | `selector` (rows), `fields: { name → spec }`    |
| `assert`     | `assert: <selector>`        | `selector`, `text_contains`, `url_contains`     |
| `eval`       | `eval: <js>`                | `script`                                        |

`extract`'s `fields` entries are either a CSS selector string (returns
`textContent`), or `{ selector, attr }` to read an attribute.

## Protocol

Line-framed JSON, one message per line, on stdin/stdout. `stderr` is treated as
opaque diagnostics by `wb` and printed dimmed to the user's terminal.

### Handshake (on spawn)

```
wb  →  {"type": "hello", "wb_version": "...", "protocol": "wb-sidecar/1"}
wb  ←  {"type": "ready", "runtime": "wb-browser-runtime", "version": "...",
        "protocol": "wb-sidecar/1", "supports": ["goto", "click", "fill", ...]}
```

### Slice

```
wb  →  {"type": "slice", "session": "airbase", "verbs": [...],
        "line_number": 42, "section_index": 3}
wb  ←  {"type": "slice.session_started", "session": "airbase",         (0..1, first slice per session)
        "session_id": "abc123", "live_url": "https://..."}
wb  ←  {"type": "verb.complete", "verb": "click", "summary": "..."}      (0..n)
wb  ←  {"type": "verb.failed", "verb": "click", "error": "..."}          (0..n)
wb  ←  {"type": "slice.complete"}  OR  {"type": "slice.failed", "error": "..."}
```

### Lifecycle event passthrough

Any `slice.<suffix>` event the sidecar emits (other than the terminal
`slice.complete` / `slice.failed` / `slice.paused`) is forwarded by `wb` to
the callback stream as a lifecycle event:

- `slice.session_*`  →  `session.*`   (run-scoped, e.g. live URL ready)
- `slice.<other>`    →  `step.<other>` (block-scoped, e.g. `slice.network_idle`)

The full event payload (minus `type`) is merged into the callback envelope, so
new fields ship without a `wb` release. See `src/sidecar.rs` for the dispatcher.

### Shutdown

```
wb  →  {"type": "shutdown"}
```

Sidecar exits 0.

## Roadmap

- v0.1 — protocol skeleton (echo only)
- v0.2 — `slice.session_started` event with stub URL
- v0.3 — Browserbase + playwright-core, real `goto/fill/click/wait_for/extract/assert` (this)
- v0.4 — `act:` recovery via Stagehand, `slice.recovered` events
- v0.5 — `wait_for_mfa` / `wait_for_email_otp` emitting `slice.paused` with
  `resume_url`
