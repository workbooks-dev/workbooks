# wb-browser-runtime

Browser sidecar for `wb` — deterministic Playwright slices with AI recovery.

> **Status:** skeleton. Speaks the JSON-over-stdio protocol and acknowledges
> verbs without running any browser code. This exists so `wb`'s `browser`
> runtime dispatch can be exercised end-to-end while the real Playwright /
> Stagehand / Browserbase integration is built out.

## Install (local dev)

```bash
cd runtimes/browser
npm link          # exposes `wb-browser-runtime` on $PATH
```

Or set `WB_BROWSER_RUNTIME=/absolute/path/to/bin/wb-browser-runtime.js` for a
specific run.

## Usage

```bash
WB_EXPERIMENTAL_BROWSER=1 wb run examples/browser-demo.md
```

See `examples/browser-demo.md` in the repo root for a minimal workbook that
exercises the protocol.

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
wb  ←  {"type": "verb.complete", "verb": "click", "summary": "..."}      (0..n)
wb  ←  {"type": "verb.failed", "verb": "click", "error": "..."}          (0..n)
wb  ←  {"type": "slice.complete"}  OR  {"type": "slice.failed", "error": "..."}
```

### Shutdown

```
wb  →  {"type": "shutdown"}
```

Sidecar exits 0.

## Roadmap

- v0.1 — protocol skeleton (this)
- v0.2 — Playwright + Browserbase context support, real `goto/click/fill/...`
- v0.3 — `act:` recovery via Stagehand, `slice.recovered` events
- v0.4 — `wait_for_mfa` / `wait_for_email_otp` emitting `slice.paused` with
  `resume_url`
