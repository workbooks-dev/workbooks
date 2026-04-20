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

## Optional: session recording (rrweb + CDP screencast)

Each browser session can be recorded two ways and uploaded to a consumer
endpoint at session close. Recording is **off by default** — set
`WB_RECORDING_UPLOAD_URL` to turn it on.

| Env var                            | Default    | Purpose                                          |
|------------------------------------|------------|--------------------------------------------------|
| `WB_RECORDING_UPLOAD_URL`          | *(unset)*  | POST target. Supports `{run_id}` / `{kind}` placeholders. Unset disables recording entirely. |
| `WB_RECORDING_UPLOAD_SECRET`       | *(unset)*  | Sent as `Authorization: Bearer <…>`. Required when upload URL is set. |
| `WB_RECORDING_RUN_ID`              | *(auto)*   | Explicit run id. Falls back to `TRIGGER_RUN_ID`, then a UUID generated at boot. |
| `WB_RECORDING_SCREENCAST_FPS`      | `5`        | CDP screencast frame rate.                        |
| `WB_RECORDING_SCREENCAST_QUALITY`  | `60`       | JPEG quality (0–100).                             |
| `WB_RECORDING_RRWEB`               | `1`        | Set `0` to skip rrweb even if recording is on.    |
| `WB_RECORDING_VIDEO`               | `0` if no `ffmpeg` | Set `0` to skip video even if `ffmpeg` is present. |

Artifacts are two parallel POSTs per session, `kind ∈ {rrweb, video}`:

- **rrweb** — gzipped JSON (`application/json+gzip`) — `{ run_id, session, event_count, events: [...] }`. DOM mutations + input events captured from every page; defaults mask all inputs for PII.
- **video** — VP9 WebM (`video/webm`) — encoded from JPEG screencast frames via `ffmpeg`. Requires `ffmpeg` on `$PATH` (droplet install: `apt-get install -y ffmpeg`). If `ffmpeg` is missing the video kind silently disables and rrweb continues alone.

Each POST carries headers `Authorization: Bearer <secret>`,
`X-WB-Run-Id`, `X-WB-Recording-Kind`, `X-WB-Session`.

### Callback events

`wb` forwards `slice.recording.*` events emitted by the sidecar as
`step.recording.*` on the callback stream:

- `step.recording.started` — once per session, payload includes `run_id`, `kinds`.
- `step.recording.uploaded` — on 2xx PUT, payload includes `kind`, `bytes`.
- `step.recording.failed` — on network/ffmpeg/upload error, payload includes `kind`, `status?`, `reason`. Non-fatal: the slice still completes.

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
- v0.3 — Browserbase + playwright-core, real `goto/fill/click/wait_for/extract/assert`
- v0.4 — rrweb + CDP screencast recording, uploaded to a consumer endpoint (this)
- v0.5 — `act:` recovery via Stagehand, `slice.recovered` events
- v0.6 — `wait_for_mfa` / `wait_for_email_otp` emitting `slice.paused` with
  `resume_url`
