# Feature Requests

## Fenced-block info-string flags: `{no-run}` and `{silent}`

### TL;DR

Teach the parser to recognize two info-string flags on executable fenced blocks:

- **`{no-run}`** — render as a normal fenced block in documentation tools but do **not** execute and do **not** count toward `blocks.total`. For illustrative snippets inside a runbook.
- **`{silent}`** — execute normally but suppress `step.complete` / `step.failed` callback events for this block. The block still counts toward `blocks.total`, its output still appears in the final JSON report, but it doesn't show up in the notify stream.

Both go in the info string alongside the language and any existing attributes, e.g. ` ```bash {no-run} ` or ` ```browser {silent} `.

### Motivation

We're adopting a "dual-audience runbook" pattern for finance ops SOPs. The runbook body is human-readable prose for a non-technical operator; each step has an adjacent `<details>` block containing the executable automation. `wb` happily parses fenced blocks inside `<details>` (confirmed) — but two cases leak into the pipeline that shouldn't:

1. **Illustrative snippets.** An "Automation Opportunities" section at the bottom of many runbooks sketches future API calls or data shapes. These are documentation, not runnable — but if the author writes them as ` ```bash ` (because that's what triggers syntax highlighting in the docs UI), `wb` will execute them on every run. Today we have to either mis-tag them as ` ```text ` (loses highlighting) or comment them out in prose.
2. **Setup boilerplate.** The first block in a runbook is often a bash cell that loads env vars and sets `MONTH=…` / `RUN_URL=…`. It's required for the run to function, but it emits a `step.complete` event with trivial stdout that just clutters the notify stream and the website UI. The meaningful events are steps 2-N.

`{no-run}` fixes (1) without losing highlighting or forcing a comment-out dance. `{silent}` fixes (2) without hiding the block from the author or changing execution semantics.

### Proposed shape

**Parser side** (`src/parser.rs`, around `extract_sections` at `:244`): after reading `language = line[3..].trim()`, split off any `{...}` attribute cluster. Treat the first token as language; parse flags inside the braces. Attach them to the `CodeBlock` / `BrowserSliceSpec` struct as `skip_execution: bool` and `silent: bool`.

Shape suggestion:

```
```bash {no-run}
```bash {silent}
```browser {silent}
```browser {no-run, silent}     # combining is fine; no-run wins (nothing to silence)
```

**Executor side** (`src/executor.rs`):

- `no_run == true` → skip the block entirely. Do not include it in `blocks.total`. Do not call the callback for it. `results[]` in the final JSON may still include a `status: "skipped"` entry so the report is complete, or simply omit it — either is fine, we lean toward a skipped entry for debuggability.
- `silent == true` → execute the block as normal. Include it in `blocks.total`. Do not call `callback.step_complete` / `callback.step_failed` for this block. Do include it in `results[]` in the final JSON report. On failure of a silent block, **do** still emit `run.failed` at the end — silence is per-block, not per-run.

**Non-goals.** We don't want per-block-stream filtering (e.g. "emit to Redis but not HTTP"). `silent` is binary: all callback sinks or none. Keep it simple.

### Open questions

1. **Attribute syntax.** We're suggesting `{no-run}` and `{silent}` (pandoc / markdown-attributes style). If `wb` already has a convention for info-string attributes (e.g. `bash env=production`), match that instead. We haven't seen one in `src/parser.rs`, but flagging for consideration.
2. **Does `{no-run}` suppress parsing errors?** If a `{no-run}` block has malformed YAML (for a documented browser snippet), should wb still log a parse warning? Our preference: yes, warn but don't fail — the author probably wants to see the error. But skip it quietly if both `{no-run}` and `{silent}` are present (treat it as pure documentation).
3. **Does `{silent}` affect step-level progress?** `blocks.total` and `blocks.done` in the JSON report are currently driven by execution. If a silent block counts toward `total`, the website UI progress bar still advances ("5/8") — good. If it doesn't count, the progress bar matches what the user sees in the notify stream — also good, and probably more intuitive. We lean: silent counts toward progress but doesn't emit the event. Open to the reverse.

### Adoption path

1. Parse the attribute cluster in `extract_sections` and attach flags to the block structs. No behavior change yet — gate both behind a `--experimental-block-flags` CLI flag while the semantics settle.
2. Implement `no-run` skip + skipped-result emission.
3. Implement `silent` suppression in the callback path only.
4. Stabilize, remove the flag, document in `SKILL.md`.

### Context: why now

The runbook-UI work (xatabase website) now renders a pause banner, a recovered-selector counter, and per-block execution results in the notify stream. Without these flags, every illustrative snippet in the "Automation Opportunities" tail of a runbook would execute on every run, and every trivial setup cell would emit a noisy event. The cleanest way to avoid polluting the stream is at the source — the author marks documentation as documentation and setup as setup, and `wb` respects it.

## Browser session recording: rrweb + CDP screencast

### TL;DR

Teach `wb-browser-runtime` (the Playwright sidecar at `runtimes/browser/bin/wb-browser-runtime.js`) to record every live browser session two ways in parallel, and upload both artifacts to a consumer-defined HTTP endpoint at session close:

- **rrweb** — inject `rrweb-record` via `page.addInitScript` so every page auto-records DOM mutations + input events onto `window.__wbRrwebEvents`. Collect on navigation + at session end, gzip, PUT.
- **CDP screencast** — open a `newCDPSession(page)` per page, call `Page.startScreencast` at configurable fps/quality, buffer JPEG frames, encode to WebM with `ffmpeg` at session end, PUT.

Both artifacts go to the same upload endpoint, keyed by the existing `TRIGGER_RUN_ID` env var, with Bearer-token auth.

### Motivation

The xatabase run page embeds Browserbase's live-view iframe during an active session — works great while the session is running, but the live URL expires the moment the session ends. Browserbase's post-session replay is dashboard-only (requires Clerk auth, no iframe-embeddable URL, rrweb API is deprecated, no video download endpoint). Operators revisiting a completed run page today see "Browser session ended" with no way to see what actually happened.

We need a first-party recording that:

1. Survives session close (stored on our own R2, not Browserbase's dashboard).
2. Plays back in our own UI with no vendor branding anywhere.
3. Is rich enough to diagnose why a runbook failed — which means capturing both DOM-level detail (rrweb, for selector churn and input replay) and pixel-accurate frames (CDP, for canvas/PDF/iframe rendering where rrweb goes blind).

rrweb alone misses the edge cases that caused Browserbase themselves to move off DOM-based recording. CDP alone is big and opaque for diagnostics — you can see *what* happened but not *why the selector missed*. Running both is cheap (different layers, no interference) and gives us belt-and-braces.

The consumer side — a Cloudflare Worker route with an R2 binding — is already built at `POST /api/runs/:run_id/recording/:kind` in the xatabase website repo. This feature request is specifically for the sidecar instrumentation that produces the artifacts.

### Proposed shape

**New env vars** (all optional; feature is off unless the upload URL is set):

- `WB_RECORDING_UPLOAD_URL` — template with `{run_id}` and `{kind}` placeholders, e.g. `https://xata.paracord.sh/api/runs/{run_id}/recording/{kind}`. When unset, recording is disabled entirely (no rrweb injection, no screencast).
- `WB_RECORDING_UPLOAD_SECRET` — sent as `Authorization: Bearer <…>`. Required when the upload URL is set.
- `WB_RECORDING_RUN_ID` — explicit override. Defaults to `TRIGGER_RUN_ID` (already set by the hermes-manager consumer), falls back to a ULID generated at sidecar boot. Only the override is exposed so non-trigger callers (tests, one-off `wb` invocations) can still record.
- `WB_RECORDING_SCREENCAST_FPS` — default `5`.
- `WB_RECORDING_SCREENCAST_QUALITY` — default `60` (0-100 JPEG quality).
- `WB_RECORDING_RRWEB` / `WB_RECORDING_VIDEO` — individual kill switches (`0` to disable just one). Both default on when upload URL is set.

**Sidecar changes** (`wb-browser-runtime.js`):

1. **At session creation** — extend `ensureSession()` (around `:115`). After `await browser.newContext()`:
   - Bundle `rrweb-record/dist/rrweb-record.min.js` into the sidecar at build time (vendored in `runtimes/browser/vendor/rrweb-record.min.js` — ~30KB minified). Read once at sidecar boot.
   - `await context.addInitScript({ content: rrwebSource + ';' + bootstrap })` where `bootstrap` starts recording and pushes events onto `window.__wbRrwebEvents = []`.
   - `const cdp = await context.newCDPSession(page)`; `await cdp.send('Page.startScreencast', { format: 'jpeg', quality, everyNthFrame: 1 })`; accumulate frames in a ring buffer (bounded by e.g. 10 min * 5fps = 3000 frames) on `cdp.on('Page.screencastFrame', …)`; ack each frame so Chrome keeps streaming.
   - Screencast persists across navigations within the same `page`; rrweb's init-script auto-reinstalls on each new document.

2. **Per-navigation event collection** — before any `goto` that changes origin, drain `window.__wbRrwebEvents` into a sidecar-side buffer via `page.evaluate(() => { const e = window.__wbRrwebEvents; window.__wbRrwebEvents = []; return e; })`. Events are small — order of KBs per page.

3. **At session end** — new `flushRecording(session)` called from `shutdown()` (around `:372`) BEFORE `browser.close()`:
   - Final rrweb drain via `page.evaluate(…)`.
   - `cdp.send('Page.stopScreencast')`; shell out to `ffmpeg -framerate ${fps} -i -` (stdin pipe of JPEG frames) `-c:v libvpx-vp9 -b:v 1M -deadline realtime out.webm`; capture stdout bytes.
   - Gzip the rrweb events (they're JSON — typically 5-10x compressible).
   - Two parallel `fetch(uploadUrl, { method: 'POST', headers: { 'Authorization': 'Bearer …', 'Content-Type': '…' }, body: … })` calls, one per kind. Fire-and-forget timeouts at ~30s — don't block shutdown longer than that.

4. **Emit new callback events** for observability:
   - `slice.recording.started` when screencast opens + rrweb is injected (once per session).
   - `slice.recording.uploaded` on successful PUT (per kind).
   - `slice.recording.failed` on upload error (per kind) — carries `kind`, `status`, `reason`. Failure doesn't fail the slice.

### Dependencies

- `rrweb-record` — vendored as a file, no npm install (matches the minimal-deps style of the current sidecar; only `playwright-core` is an npm dep today).
- `ffmpeg` — must be on `$PATH` of the agent running the sidecar. Add a preflight check that logs a warning if missing and disables just the video kind (rrweb still uploads). Droplet install: `apt-get install -y ffmpeg`.

### Open questions

1. **Frame buffering vs. streaming encode.** Option A (simplest): buffer all JPEG frames in memory, pipe to ffmpeg stdin at session end. Memory bound: 3000 frames × ~30KB ≈ 90MB worst case. Option B: spawn ffmpeg at session start with a named pipe (`mkfifo`) and stream frames as they arrive. Lower memory, more moving parts. I'd start with A and revisit if long sessions OOM.

2. **rrweb sampling.** Default rrweb records every DOM mutation. On a noisy page (e.g. finance dashboards with polling widgets), this can generate 1000s of events. rrweb supports `sampling: { scroll: 150, input: 'last' }` etc. — worth setting sensible defaults but exposing via env var.

3. **Interaction with `--checkpoint` replay.** If wb is resumed via `--checkpoint`, does the sidecar open a fresh session or reconnect to the persisted one? Currently session cache is in-process — a resume starts a new sidecar process, so it will create a new Browserbase session. Recording artifacts for the pre-checkpoint portion will have uploaded when the original process released. The replay will produce a second set. Question: should we concatenate on the Worker side (have `/recording/:kind` append instead of overwrite)? Or keep it one-file-per-sidecar-process and tag with a segment index in the object key (`runs/<id>/video.1.webm`, `.2.webm`)? Lean: segment index. The playback UI can stitch.

4. **PII.** rrweb records text input verbatim by default. We already redact `fill` values from stdout summaries — but rrweb bypasses the verb layer and captures keystroke events at the DOM level. Options: (a) rrweb's `maskAllInputs: true` (loses useful context for debugging), (b) `maskInputFn` with a list of sensitive selectors, (c) add a `sensitive:` frontmatter array on the workbook that gets translated into a rrweb mask config. Lean: (a) on by default, (c) as an escape hatch for known-safe pages.

### Adoption path

1. **Boot-time preflight** — detect `rrweb-record` vendor file, detect `ffmpeg`, log enabled capabilities. No feature flag — if upload URL is set and deps are present, recording is on.
2. **rrweb-only first** — ship the DOM-events pipeline end-to-end (inject, collect, gzip, PUT). Lower risk; no subprocess, no binary size concerns.
3. **CDP screencast second** — add ffmpeg pipeline once rrweb is stable. Behind an independent kill switch.
4. **Segmented uploads for checkpoint-resumed runs** — only if/when checkpoint replay becomes a common path.

### Context: why now

The xatabase website stripped all "Browserbase" branding from its run page and removed the external `browserbase.com/sessions/<id>` replay links (ref commits on `main` 2026-04-20). The run page currently shows "Browser session ended" with no replay, which is a regression in operator-facing diagnostics — before the strip, the external link at least took you somewhere. A first-party recording closes the loop: vendor-hidden UI, inline playback, diagnostics rich enough to refine failing runbooks. The Worker storage routes (`POST/GET /api/runs/:run_id/recording/:kind`) and the R2 bucket (`xata-paracord-runbooks-output`) are already in place awaiting producers.

## Sidecar shutdown SIGKILLs before flushRecording can upload (bug)

### TL;DR

`src/sidecar.rs:88-96` writes a `shutdown` frame to the sidecar and then immediately calls `child.kill()` (SIGKILL on Unix), giving the sidecar zero time to act on the shutdown. With `wb-browser-runtime@0.4.0`'s recording feature, this means `flushRecording()` (rrweb drain + ffmpeg finalize + upload) never runs — the sidecar gets terminated mid-async-enqueue. Field-confirmed: `slice.recording.started` fires during the slice, but `slice.recording.uploaded` / `slice.recording.failed` never appear in the notify stream, matching the code path being killed.

### Current behavior

```rust
impl Drop for Sidecar {
    fn drop(&mut self) {
        let _ = writeln!(self.stdin, "{}", json!({ "type": "shutdown" }));
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
```

### Proposed shape

Give the sidecar a bounded window to exit cleanly before falling back to SIGKILL. 45s covers the sidecar's own 30s per-upload timeout plus ffmpeg finalize plus safety margin:

```rust
use std::time::{Duration, Instant};

impl Drop for Sidecar {
    fn drop(&mut self) {
        let _ = writeln!(self.stdin, "{}", json!({ "type": "shutdown" }));
        let _ = self.stdin.flush();
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() >= deadline => break,
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(_) => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
```

### Open questions

1. **Configurable timeout.** 45s is fine for recording uploads but punitive for test harnesses that want fast teardown. Could be an env (`WB_SIDECAR_SHUTDOWN_TIMEOUT_SECS`) defaulting to 45.
2. **Shutdown observability.** Currently a hung sidecar just sits invisible. Worth logging `[sidecar] waiting Ns for clean shutdown...` on any wait over ~2s so operators know why `wb` feels slow to exit.
3. **Interaction with `--checkpoint`.** A checkpointed pause that later resumes spawns a new sidecar — the previous one's recording flush still has to complete before the parent exits, or the first segment's artifacts are lost. The loop above handles this correctly, but worth verifying once the segmented-upload story (from the recording feature-request above) is decided.

### Context: why now

Blocking end-to-end on the recording pipeline. The producer (sidecar) is correct — `flushRecording()` at `wb-browser-runtime.js:417-501` does the drain/encode/upload sequence and emits lifecycle events. The consumer (Worker routes + R2 + D1 flags + BrowserReplay UI) is deployed and ready. The only reason no artifact ever lands is this SIGKILL race. Fixing the shutdown wait is the last step for end-to-end recording to work.
