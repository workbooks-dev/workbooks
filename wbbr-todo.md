# wb-browser-runtime — improvement backlog

Synthesis of a 7-agent review of `runtimes/browser/bin/wb-browser-runtime.js` (970 lines) and the Rust-side integration at `src/sidecar.rs`. Ranked by impact. Line numbers reference the sidecar JS file unless noted.

## Critical

- [x] **Path traversal in artifact reads + screenshot paths** (security) — *shipped*
  - Tightened `ARTIFACT_RE` to `[A-Za-z_][A-Za-z0-9_-]*` (no dots).
  - Added `resolveInside(dir, candidate)` that rejects traversal and absolute paths; `readArtifact()` uses it.
  - `screenshot:` now rejects absolute paths and any path escaping `$WB_ARTIFACTS_DIR`; also `mkdir -p` the target dir before writing. **Breaking:** absolute-path escape hatch removed by design.

- [x] **Unbounded rrweb event buffer** (perf) — *shipped*
  - Drop-oldest ring behavior in both push sites (binding callback + final drain).
  - Cap configurable via `WB_RECORDING_RRWEB_MAX_EVENTS` (default 50,000). One-shot warning logged on first overflow; `dropped` field added to uploaded rrweb payload so consumers can detect truncation.

- [x] **Session zombies on partial creation failure** (resilience) — *shipped*
  - `ensureSession` now wraps everything after `bbCreateSession` in try/catch. On failure: close browser (if connected), `sessions.delete(name)`, and `bbReleaseSession(created.id)` before re-throwing.

- [x] **Unhandled exceptions in enqueue chain never emit `slice.failed`** (resilience) — *shipped*
  - `handleSlice` body wrapped in top-level try/catch that always emits `slice.failed`.
  - `enqueue(fn, kind)` now emits a terminal `slice.failed` from the chain's own `.catch` as a belt-and-suspenders guard if the handler ever throws past its own try.
  - `unhandledRejection` logged so background promise failures surface.

- [x] **ffmpeg orphan on hard crash + 15s shutdown race + missing signal handlers** (recording) — *shipped*
  - Timeout default raised to 30s, configurable via `WB_RECORDING_FFMPEG_TIMEOUT_MS`.
  - Non-zero ffmpeg exit codes now mark the video as failed (skip upload of corrupt webm) and emit `slice.recording.failed` with `reason: ffmpeg_exit_code_N` / `ffmpeg_timeout_Nms` / `finalize_error:…`.
  - `SIGTERM` / `SIGINT` / `SIGHUP` handlers added (sidecar previously had none → parent-initiated kill skipped `shutdown()` entirely, orphaning ffmpeg AND Browserbase sessions). Verified with a local SIGTERM probe: `[shutdown] received SIGTERM` → exit 0.

- [ ] **Cross-origin rrweb gap** (recording) — *deferred, needs reproduction*
  - One reviewer claimed `context.addInitScript` doesn't carry across cross-origin navigations in isolated BrowsingContexts; this contradicts Playwright's documented context-wide scoping, and the reviewer didn't supply a repro. Skipped until we can observe partial recordings on a real OAuth flow. If confirmed, fix via `page.on('framenavigated')` re-bootstrap with per-frame event aggregation.

## High

- [x] **Retry/backoff on Browserbase API** — *shipped*
  - New `retryableFetch(url, opts, label, { timeoutMs })` helper: 3 attempts, 100ms/500ms backoffs, retries only on network throw / 5xx / 429, per-attempt AbortController with 30s default timeout. Wired into `bbCreateSession`, `bbGetLiveUrl`, `bbReleaseSession`.

- [x] **Retry on recording uploads** — *shipped*
  - `uploadArtifact()` now uses `retryableFetch` with a 30s per-attempt budget (≤90s worst case).
  - Duplicate-upload mitigation deferred: today `flushRecording()` runs only once per process (at shutdown) so re-upload isn't reachable. Whoever introduces per-slice flushing should add clear-on-success buffer semantics in the same PR.

- [x] **Atomic writes for `screenshot` + `save`** — *shipped*
  - Both verbs now write to `${full}.${pid}.${rand}.tmp`, `fs.rename()` atomically, and *only then* emit `slice.artifact_saved`. Failed writes clean up the `.tmp` and re-throw.

- [x] **Redact secrets in thrown error messages** — *shipped*
  - `expand()` now collects every `{{ env.X }}` / `{{ artifacts.X }}` value (≥3 chars) into a per-slice `secrets` Set threaded through `sliceCtx`.
  - `scrubSecrets(msg, secrets)` replaces each collected secret with `«***»`. Applied to the `error` field of every `verb.failed` / `slice.failed` / `session start failed` / top-level guard frame. Secrets never cross the stdio boundary in Playwright error strings, even when they echo URL/script/assertion inputs.

- [x] **CDP screencast backpressure** — *shipped*
  - Frame handler honors `ff.stdin.write()`'s return value. On `false`, waits for `drain` (or `close`/`error`) before `Page.screencastFrameAck` — Chrome throttles upstream instead of Node heap growing. 5s fail-open so a wedged ffmpeg can't stall the protocol.

## Medium

- [x] **Monolithic 970-line dispatch** — *shipped*
  - Extracted `runtimes/browser/verbs/*.js` — one file per verb, each exporting `{ name, primaryKey, execute(page, args, ctx) }`. Registry in `verbs/index.js` builds `SUPPORTS` + default-key lookup + dispatch from the module list automatically. Entry point shrank from 1394 → 498 LOC.
  - Extracted `runtimes/browser/lib/session-manager.js` — owns the name→SessionInfo cache, adds in-flight-create dedup (two concurrent `ensure("x")` calls share one bbCreateSession), and provides per-session dispatch chains (`enqueueOn`) + global drain (`drainAll`) for shutdown.
  - Extracted `runtimes/browser/lib/recording-manager.js` — full rrweb + screencast lifecycle (`start`/`flush`) with the config as a constructor-scoped object and `loadRecordingConfig()` exported separately. Per-session state still lives on `info.recording` so SessionManager stays a plain Map.
  - Extracted `runtimes/browser/lib/http.js` (retryableFetch + safeText, shared between Browserbase REST and recording uploads), `lib/io.js` (send + log), `lib/util.js` (resolveInside + redact + artifact-name helpers).

- [x] **`SUPPORTS` array drifts from implementation** — *shipped*. `SUPPORTS` is now `VERBS.map(v => v.name)` in `verbs/index.js`. Adding a verb = dropping a new file + adding one import-and-array-entry line. No third list.

- [x] **Missing `{{ env.X }}` / `{{ artifacts.X }}` becomes empty string** — *shipped*
  - New `WB_SUBSTITUTION_ON_MISSING` env var (`warn` | `error` | `empty`, default `warn` for back-compat). Strict `error` mode throws a clean `substitution: env.X is not set` / `substitution: artifacts.X is not set` inside `runVerb`'s try, which flows through the existing `verb.failed` / `slice.failed` path with scrubbed messages.
  - Fixed the misleading "leaving placeholder" comment — the code was always returning `""`, not a visible placeholder. Log now says "substituting empty string" honestly.
  - Unified env + artifact missing branches through a single `handleMissingSubstitution(kind, name)` helper so the policy lives in one place.
  - README updated with the policy table.

- [x] **README says PUT; code uses POST** — *shipped*. Fixed README:80 to match the code, which has always POSTed.

- [x] **FPS/quality unclamped + no duplicate-frame dedup** — *shipped*
  - `WB_RECORDING_SCREENCAST_FPS` clamped to [1,30], `WB_RECORDING_SCREENCAST_QUALITY` clamped to [10,95]; one-shot log on boot if the operator asked for an out-of-range value.
  - `Page.screencastFrame` handler compares `frame.data` (base64 string) to the previous frame and skips the `ff.stdin.write` + bookkeeping when they match (still acking so Chrome keeps streaming). One-shot `[recording] dedup active` log after 100 skipped frames. Reduces WebM bloat and fixes playback pacing on static pages.

- [x] **Sequential global chain serializes unrelated sessions** — *shipped*
  - Main loop's `enqueue` replaced with `SessionManager.enqueueOn(name, fn)` — a `Map<name, Promise>` keyed by slice `session` name. Slices against distinct sessions now run in parallel; same-session slices still serialize.
  - Safety: `SessionManager.ensure()` dedups in-flight creates so two concurrent ensures for the same name share one `bbCreateSession` instead of racing to burn two Browserbase sessions. Dedup shipped in Phase 2 ahead of the chain change so the per-session dispatch was a single-site swap rather than a surgery.
  - `drainAndShutdown()` awaits `sessions.drainAll()` (Promise.allSettled across every current chain) before closing browsers — signal/stdin-close paths still reach `shutdown()` cleanly. Verified with a SIGTERM probe: `[shutdown] received SIGTERM` → exit 0.

- [x] **Artifact reads are per-verb + sync** — *shipped*
  - Per-slice `Map<name, string|null>` on `sliceCtx` threaded through `expand()` → `readArtifact(name, cache)`. First read hits disk, subsequent verbs in the same slice hit the cache. `null` sentinel distinguishes "cached miss" from "never looked up" so the missing-value policy still fires.
  - Freshness preserved because the cache is slice-scoped — a bash cell that rewrites the file between slices is seen by the next slice's first verb.
  - Did NOT convert to async `readFile`: the read sits inside `String.prototype.replace` callbacks in `expand()`, which is synchronous by contract. Making it async would cascade into an async rewrite of the whole substitution pipeline without a clear win until an artifact is >1MB. Caching gives the common-case speedup without that churn.

- [ ] **PauseInfo + resume flow undefined for v0.7** — unchanged. Still waiting on a design decision (sidecar polls vs external agent delivers; `restore.signal` schema) before implementing pause verbs.

## Low / hygiene

- [x] **No tests** — *shipped*
  - `runtimes/browser/lib/stub-page.js` — in-memory fake `Page` (~90 LOC) that records every method call and returns canned responses (screenshot Buffer, extract rows, eval result, $ handles). Also exports `captureSendFrames()` to intercept JSON frames written by `lib/io.js` `send` during a test.
  - `runtimes/browser/test/verbs.test.js` — 34 tests covering the registry shape (SUPPORTS ordering, every verb exports `{name, primaryKey, execute}`, `defaultKey`/`verbName`/`arg` helpers) and every verb end-to-end. Screenshot + save tests use real tmpdirs and assert atomic-write semantics (no leftover `.tmp`) and the emitted `slice.artifact_saved` frame shape.
  - `runtimes/browser/test/session-manager.test.js` — 11 tests for the cache, in-flight create dedup (two concurrent `ensure("x")` calls share one createFn), retry-after-failed-create (no poisoned entry), per-session `enqueueOn` parallelism across distinct names, `drainAll` allSettled semantics, Map-like iteration.
  - Wired `npm test` → `node --test` (auto-discovery). 45/45 passing under Node 24.15.0. No Browserbase credentials, no network, <100ms total.

- [x] **Per-verb timing / structured logging / log levels** — *shipped*
  - `WB_LOG_LEVEL` (trace|debug|info|warn|error, default `info`) filters stderr output. `lib/io.js` exports `logTrace` / `logDebug` / `log` / `logWarn` / `logError`; `log()` stays info-level so existing call sites need no reclassification. Invalid values fall back to `info` with a one-shot warning. 6 tests in `test/io.test.js` verify each level's inclusion boundary.
  - `verb.complete` and `verb.failed` frames include `duration_ms` (Date.now() delta around `runVerb`).
  - `slice.session_started` gained a `timings` object: `allocate_ms` (bbCreateSession), `connect_ms` (bbGetLiveUrl + connectOverCDP), `page_ready_ms` (newContext/newPage), `total_ms`. Chose a single frame with breakdown over three separate milestone frames — cleaner for consumers and avoids growing the slice.session_* namespace inconsistently.
  - README documents WB_LOG_LEVEL, duration_ms, and the timings shape.

- [ ] **rrweb `maskAllInputs` masks values only** — labels, placeholders, aria-labels, and DOM structure are still captured. README's "mask all inputs for PII" overclaims. Document honestly; expose a `maskCustom` hook for known-sensitive selectors.

- [ ] **Protocol v1 has no capability negotiation or escape syntax** — `wb-sidecar/1` is cosmetic. Add `minProtocolVersion` + a `supports` feature list in `ready`, and reserve `\{\{` for literal template braces.

- [x] **Video upload buffers the whole payload** — *shipped*
  - `uploadArtifact` now accepts either a Buffer (rrweb path, unchanged) or a `{ factory, bytes, cleanup }` descriptor. Video flow passes a factory that returns `fs.createReadStream(videoPath)` per attempt, so a multi-hundred-MB WebM streams straight from disk into fetch instead of being slurped into a Buffer.
  - `retryableFetch` learned a `bodyFactory` option — required because the first attempt drains the stream, so retries need a fresh one. Sets `duplex: "half"` (undici requirement for streaming bodies).
  - Video file unlink moved to `uploadArtifact`'s `finally` so we keep the source until upload settles (success or failure). If no upload path (ffmpeg failure, etc.), `flushRecording` still cleans up eagerly.
  - rrweb kept buffered: the retry-safe streaming variant would require holding the source JSON to re-gzip on retry — same memory footprint, more code.

- [x] **Silent `flushRecording()` failures on shutdown** — *shipped*
  - rrweb pre-upload failures (final drain / gzip) now set `rrwebFailure` and emit `slice.recording.failed { kind: "rrweb", reason }` when no body is produced. Matches the existing video-failure symmetry.
  - `shutdown()`'s `flushRecording` catch now emits a `slice.recording.failed { reason: "finalize_error: ..." }` frame in addition to stderr logging, so consumers see loss instead of silently missing uploaded events.

- [x] **`wait_for` has no global per-slice deadline** — *shipped*
  - New `WB_SLICE_DEADLINE_MS` (default 120_000). `handleSlice` computes `sliceDeadline` and checks it at the top of each verb iteration; on breach, emits `slice.failed` with an abort message naming the verb index that was skipped. Rust's `SLICE_EVENT_TIMEOUT` is per-event (resets on every `verb.complete`), so a chain of `wait_for`s that each emit within 15s would otherwise run unbounded — this is the sidecar's own total-time cap. Documented in `runtimes/browser/README.md`.
  - NOT clamping per-verb timeouts to remaining deadline: a single verb with `timeout: > sliceDeadline` still runs to completion before the next iteration's check. That's an operator-configuration edge case, not the common failure mode this fixes.

## Suggested sequencing

- **Week 1 — safety:** path traversal, exception hygiene, atomic writes, error redaction.
- **Week 2 — reliability:** rrweb cap, ffmpeg hardening + retries + cross-origin, CDP backpressure.
- **Week 3 — structure:** verb modules, SessionManager, RecordingManager — unblocks v0.6/v0.7 roadmap without surgery.
- **Ongoing:** stub-mode tests + structured logging so the above doesn't regress.
