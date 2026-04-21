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

- [ ] **Monolithic 970-line dispatch** — the 144-line switch will balloon with roadmap verbs (`act:`, `wait_for_mfa`, `wait_for_email_otp`). Extract `runtimes/browser/verbs/*.js` (each exports `{ name, primaryKey, execute }`), a `SessionManager`, and a `RecordingManager` with explicit lifecycle (`start/capture/pause/flush/abort`). **This unblocks the two items below.**

- [ ] **`SUPPORTS` array drifts from implementation** — blocked on the refactor above. Without handler introspection the best we can do is a third hand-maintained list, which just moves the drift problem. Revisit once verb modules land.

- [x] **Missing `{{ env.X }}` / `{{ artifacts.X }}` becomes empty string** — *shipped*
  - New `WB_SUBSTITUTION_ON_MISSING` env var (`warn` | `error` | `empty`, default `warn` for back-compat). Strict `error` mode throws a clean `substitution: env.X is not set` / `substitution: artifacts.X is not set` inside `runVerb`'s try, which flows through the existing `verb.failed` / `slice.failed` path with scrubbed messages.
  - Fixed the misleading "leaving placeholder" comment — the code was always returning `""`, not a visible placeholder. Log now says "substituting empty string" honestly.
  - Unified env + artifact missing branches through a single `handleMissingSubstitution(kind, name)` helper so the policy lives in one place.
  - README updated with the policy table.

- [x] **README says PUT; code uses POST** — *shipped*. Fixed README:80 to match the code, which has always POSTed.

- [x] **FPS/quality unclamped + no duplicate-frame dedup** — *shipped*
  - `WB_RECORDING_SCREENCAST_FPS` clamped to [1,30], `WB_RECORDING_SCREENCAST_QUALITY` clamped to [10,95]; one-shot log on boot if the operator asked for an out-of-range value.
  - `Page.screencastFrame` handler compares `frame.data` (base64 string) to the previous frame and skips the `ff.stdin.write` + bookkeeping when they match (still acking so Chrome keeps streaming). One-shot `[recording] dedup active` log after 100 skipped frames. Reduces WebM bloat and fixes playback pacing on static pages.

- [ ] **Sequential global chain serializes unrelated sessions** — blocked on `SessionManager` from the refactor above. A per-session chain `Map<name, Promise>` alone isn't safe: two concurrent slices for the same session name would both race `bbCreateSession`. The fix needs both per-session chains AND an in-flight-create dedup via `Map<name, Promise<SessionInfo>>`, which is what `SessionManager` provides.

- [x] **Artifact reads are per-verb + sync** — *shipped*
  - Per-slice `Map<name, string|null>` on `sliceCtx` threaded through `expand()` → `readArtifact(name, cache)`. First read hits disk, subsequent verbs in the same slice hit the cache. `null` sentinel distinguishes "cached miss" from "never looked up" so the missing-value policy still fires.
  - Freshness preserved because the cache is slice-scoped — a bash cell that rewrites the file between slices is seen by the next slice's first verb.
  - Did NOT convert to async `readFile`: the read sits inside `String.prototype.replace` callbacks in `expand()`, which is synchronous by contract. Making it async would cascade into an async rewrite of the whole substitution pipeline without a clear win until an artifact is >1MB. Caching gives the common-case speedup without that churn.

- [ ] **PauseInfo + resume flow undefined for v0.7** — unchanged. Still waiting on a design decision (sidecar polls vs external agent delivers; `restore.signal` schema) before implementing pause verbs.

## Low / hygiene

- [ ] **No tests** — highest-ROI addition: `--stub-mode` / `WB_STUB_MODE=1` with an in-memory fake `Page` (~150 LOC in `runtimes/browser/lib/stub-page.js`) + `runtimes/browser/test/verbs.test.js` using Node's built-in test runner. Unlocks `npm test` without Browserbase credentials.

- [ ] **No per-verb timing / structured logging / log levels** — add `WB_LOG_LEVEL` (trace|debug|info|warn|error), emit `duration_ms` on `verb.complete`, attach session-lifecycle milestones (`allocated`, `connected`, `page_ready`).

- [ ] **rrweb `maskAllInputs` masks values only** — labels, placeholders, aria-labels, and DOM structure are still captured. README's "mask all inputs for PII" overclaims. Document honestly; expose a `maskCustom` hook for known-sensitive selectors.

- [ ] **Protocol v1 has no capability negotiation or escape syntax** — `wb-sidecar/1` is cosmetic. Add `minProtocolVersion` + a `supports` feature list in `ready`, and reserve `\{\{` for literal template braces.

- [ ] **gzip + upload buffer the whole payload** (~469, 535–544) — use `zlib.createGzip()` streamed into a fetch body; keep-alive is automatic on modern fetch.

- [ ] **Silent `flushRecording()` failures on shutdown** (~903–906) — errors hit stderr but no frame is emitted, so Rust executor still reports `slice.complete`. Track kind-level success; emit `slice.recording.failed` before the terminal frame if any kind dropped.

- [ ] **`wait_for` has no global per-slice deadline** (~678) — 25 × 15s waits exceed Rust's 300s `SLICE_EVENT_TIMEOUT`; sidecar keeps running after timeout. Add a configurable per-slice cap (e.g. 60–120s) that aborts remaining verbs.

## Suggested sequencing

- **Week 1 — safety:** path traversal, exception hygiene, atomic writes, error redaction.
- **Week 2 — reliability:** rrweb cap, ffmpeg hardening + retries + cross-origin, CDP backpressure.
- **Week 3 — structure:** verb modules, SessionManager, RecordingManager — unblocks v0.6/v0.7 roadmap without surgery.
- **Ongoing:** stub-mode tests + structured logging so the above doesn't regress.
