# wb-browser-runtime

Browser sidecar for `wb` — deterministic Playwright slices over a CDP-exposing
vendor. Browserbase is the default; browser-use cloud is supported via
`WB_BROWSER_VENDOR=browser-use`.

Each `browser` fenced block in a workbook arrives as one `slice` message;
this sidecar dispatches its `verbs` against a `playwright-core` `Page`
connected to a vendor-provided CDP endpoint. Sessions are cached by `session:`
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

## Vendor selection

`WB_BROWSER_VENDOR` — `browserbase` (default) or `browser-use`. Resolved once
at sidecar boot; there is no per-slice override.

### Browserbase (default)

- `BROWSERBASE_API_KEY`
- `BROWSERBASE_PROJECT_ID`

### browser-use

- `BROWSER_USE_API_KEY`
- `BROWSER_USE_PROXY_COUNTRY` *(optional)* — ISO country code for the cloud's
  built-in residential proxy (defaults to `us` on the vendor side). Set to
  `null` to disable the proxy.
- `BROWSER_USE_TIMEOUT_MIN` *(optional, 1–240)* — session TTL. Vendor default
  is 60 minutes; unused time is refunded if the session ran less than an hour.

Profile (auth state) is selected per-runbook via the `profile_id:` field on a
`browser` block — see "Profiles" below. `BROWSER_USE_PROFILE_ID` is read as a
default when the browser block omits `profile_id:`; a per-runbook `profile_id:`
always wins over the env var.

## Profiles

Some vendors expose persistent browser profiles — cookies, localStorage, saved
auth — that bind a session to a previously-logged-in identity so the
runbook can skip login. The current support matrix:

| Vendor       | Profile field | Source of profile id                         |
|--------------|---------------|----------------------------------------------|
| browser-use  | `profileId`   | `curl -fsSL https://browser-use.com/profile.sh \| sh` |
| browserbase  | n/a           | logged + ignored                             |

A runbook pins a profile via `profile_id:` on the `browser` block:

```browser
session: airbase
profile_id: 550e8400-e29b-41d4-a716-446655440000
verbs:
  - goto: https://dashboard.airbase.io/home
```

The id is an opaque UUID that the runbook generator (UI editor, codegen, or
hand-author) bakes in. Rotating the underlying auth state means re-emitting
the runbook with a fresh id — no env-var shuffle and no in-place edits at
run time. Browserbase runs ignore the field with an info-level log so the
same runbook executes against either vendor.

Verb arguments support two substitutions at dispatch time:

- `{{ env.NAME }}` — reads `process.env.NAME`. Use for static secrets injected via Doppler / the agent's env.
- `{{ artifacts.NAME }}` — reads `$WB_ARTIFACTS_DIR/NAME.txt` (falling back to `$WB_ARTIFACTS_DIR/NAME`). Use for dynamic values produced by an earlier bash cell — OTPs, magic-link URLs, export IDs, anything polled from an external system mid-run. Reads are cached for the duration of one slice; a bash cell that runs *between* slices is always picked up by the next slice's verbs.

Both forms are redacted in stdout summaries — only the verb name + selector make it into the log. Expanded values are also scrubbed from `verb.failed` / `slice.failed` error messages before they cross the stdio boundary.

**Missing-value policy.** Set `WB_SUBSTITUTION_ON_MISSING` to choose how a missing `env.X` or `artifacts.X` is handled:

- `warn` (default) — log a stderr warning and substitute an empty string; the verb continues.
- `error` — throw, failing the slice. Use in CI so a missing OTP doesn't silently dispatch an empty selector.
- `empty` — substitute empty silently (suppresses the warning).

**Slice wall-clock cap.** `WB_SLICE_DEADLINE_MS` (default `120000` = 2 min) aborts a slice if aggregate verb time exceeds it. This is an independent bound from each verb's own `timeout:` — a chain of 25 × 15s `wait_for`s all emitting events would never trip `wb`'s 300s per-event sidecar timeout, so the sidecar applies its own total-time cap. Set higher for legitimately long slices (long polling across multiple `wait_for`s).

**Log level.** `WB_LOG_LEVEL` (`trace` | `debug` | `info` | `warn` | `error`, default `info`) filters stderr diagnostic output. Existing `[recording]` / `[retry]` / `[shutdown]` lines are info-level; unknown values fall back to info with a one-shot warning.

**Per-verb + session timings.** `verb.complete` and `verb.failed` frames include `duration_ms`. `slice.session_started` includes a `timings` object with `allocate_ms` (bbCreateSession), `connect_ms` (bbGetLiveUrl + CDP connect), `page_ready_ms` (context/page setup), and `total_ms`. Graph these to see where slow sessions spend time — usually `connect_ms` on a cold Browserbase region.

## Optional: anti-detection (Browserbase only)

Targets behind Cloudflare / Kasada / DataDome (e.g. Airbase) will reject the
default Browserbase session fingerprint and serve a non-interactive challenge
page. Flip either flag on for the affected runs.

| Env var                            | Default | Purpose                                          |
|------------------------------------|---------|--------------------------------------------------|
| `BROWSERBASE_ADVANCED_STEALTH`     | *(off)* | Send `browserSettings.advancedStealth: true`. Browserbase Scale-plan-gated — API errors on lower plans. |
| `BROWSERBASE_PROXIES`              | *(off)* | Send `proxies: true`. Routes through Browserbase residential proxy pool. Incurs extra per-session cost. |

Set `=1` (or `=true`) to enable. `proxies: true` alone clears most Cloudflare
challenges; add `advancedStealth: true` on top when the target still blocks.
The sidecar logs the resolved config at session create. Ignored when
`WB_BROWSER_VENDOR=browser-use` — that vendor has stealth + residential
proxies on by default.

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
- `step.recording.uploaded` — on 2xx POST, payload includes `kind`, `bytes`.
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
| `save`       | `save: <name>`              | `name`, `value` (captures prior `extract`/`eval` when omitted) |

`extract`'s `fields` entries are either a CSS selector string (returns
`textContent`), or `{ selector, attr }` to read an attribute.

## Artifacts

`wb` exports `$WB_ARTIFACTS_DIR` to every cell — a per-run directory
(`~/.wb/runs/<run_id>/artifacts/` by default) where any cell can drop files
that later cells will read back. The browser `save:` verb is the
sidecar-side equivalent:

```yaml
- extract:
    selector: .order-row
    fields:
      id: .order-id
      total: .total
- save: orders            # writes $WB_ARTIFACTS_DIR/orders.json
```

Forms:

- `save: <name>` — captures the previous verb's JSON output (from
  `extract` or `eval`) into `<name>.json`.
- `save: { name: orders, value: { ... } }` — writes an inline value.
- `save: {}` — auto-names the file `cell-<block_index>-<rand>.json`.

Downstream bash/python cells read the file directly:

```bash
jq '.[0].id' "$WB_ARTIFACTS_DIR/orders.json"
```

When `WB_ARTIFACTS_UPLOAD_URL` is set (template supports `{run_id}` and
`{filename}`), `wb` POSTs each new artifact file after the cell that
produced it completes. Auth reuses `WB_RECORDING_UPLOAD_SECRET`
(`Authorization: Bearer <…>`); failures are logged and non-fatal.

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
- v0.4 — rrweb + CDP screencast recording, uploaded to a consumer endpoint
- v0.5 — `save:` verb + shared `$WB_ARTIFACTS_DIR` for cross-cell data (this)
- v0.6 — `act:` recovery via Stagehand, `slice.recovered` events
- v0.7 — `wait_for_mfa` / `wait_for_email_otp` emitting `slice.paused` with
  `resume_url`
