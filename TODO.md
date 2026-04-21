# TODO — wb improvements

Generated 2026-04-20 from a multi-agent audit. Items are grouped by theme, not priority within a theme. See the "Sequencing" section at the bottom for recommended order.

Status legend: `[ ]` not started · `[~]` in progress · `[x]` done · `[-]` dropped

---

## 🚨 Fix first — silent data-loss risks

- [x] **1. Atomic checkpoint + pending writes.** Shipped in `1fe0323` (write_secret_file with tmp+rename + 0o600 on Unix).
- [x] **2. Remove `checkpoint_id.as_ref().unwrap()` panics.** Shipped in `f2e21b5` (zipped-Option matches).
- [x] **3. File lock on concurrent checkpoint writes.** Shipped in `f2e21b5` (session-long flock in `run_single` + `cmd_resume`).
- [x] **4. Reserved-name blocklist for bound_vars.** Shipped in `87d462a` + defense-in-depth env-apply filter in `f2e21b5`.
- [x] **5. Verify signal-validation ordering.** Verified correct — validation happens before checkpoint mutation (`src/main.rs:2569-2586`). Bonus belt-and-braces via #4's env-apply filter.

## 🎯 Highest agent-leverage UX wins

- [x] **6. Structured error types in JSON output.** Shipped in v0.9.8 — `error_type` on `BlockResult`, JSON output, and callback `step.complete`/`checkpoint.failed` payloads. Stable tokens: `spawn_not_found`, `spawn_failed`, `nonzero_exit`, `signal_killed`, `sandbox_failed`, `read_error`, `setup_failed`, `env_file_failed`, `wait_without_checkpoint`, `pause_without_checkpoint`.
- [x] **7. Real exit-code vocabulary.** Shipped in v0.9.7 — `src/exit_codes.rs` with documented table (0 success, 1 block-failed, 2 usage, 3 workbook-invalid, 5 sandbox-unavailable, 6 checkpoint-busy, 7 signal-timeout, 42 paused).
- [x] **8. `wb inspect --json`.** Shipped in `09a8d79` — stable `{source, frontmatter, blocks[]}` shape covering code/wait/browser sections.
- [x] **9. Trace-correlation field.** Shipped in `09a8d79` — `run_id` threaded through `RunSummary`, `CallbackConfig`, and every callback payload. Resolution order: `WB_RECORDING_RUN_ID` → `TRIGGER_RUN_ID` → generated.
- [x] **10. Partial output capture on timeout/SIGKILL.** Shipped in v0.9.10 — `stdout_partial`/`stderr_partial` flags on `BlockResult`, JSON output, and callback `step.complete`/`checkpoint.failed` payloads. Timed-out blocks retain everything emitted before the kill; `error_type: "timeout"` pins them apart from clean nonzero exits. `BLOCK_TIMEOUT` is now `ExecutionContext.block_timeout` so #15's per-block timeouts can override without touching collection.
- [x] **11. Line+column + "did-you-mean" on parse/runtime errors.** Shipped in v0.9.7 — "no executable blocks" lists known runtimes + flags caveat; ENOENT on spawn now gives per-language install hints + `exec:` escape hatch. (Open follow-up: line/column for malformed frontmatter YAML.)
- [x] **12. Callback `event_version` + retries.** Shipped in v0.9.9 — `event_version: "1"` on every payload, HTTP callbacks retry 5xx + network errors with 0ms/200ms/1000ms backoff, 4xx treated as terminal. (Ordering guarantees for HTTP stay best-effort; Redis XADD already orders.)

## 🧠 Strategic bets

- [ ] **13. Pandoc-style fence attrs** — ``` ```python {#step-3 .retryable timeout=30s} ```. Canonical home for ALL future per-block config. Do this early — items 6/7/14/15 slot in cheaply afterward.
- [ ] **14. Parameterized runs.** `wb run deploy.md --param region=us-east-1`. Frontmatter declares defaults + types. Param hash feeds into checkpoint identity.
- [x] **15. Per-block `timeout`, `retry`, `continue-on-error`.** Shipped in v0.9.10 via frontmatter maps keyed by 1-based block number: `timeouts: {3: 2m}`, `retries: {3: 2}`, `continue_on_error: [4]`. Retries run with a 500ms backoff between attempts; a timeout on retry spawns a fresh session (state reset). `continue_on_error` lets a single block fail without tripping `--bail`. When fence attrs (#13) land, these same maps can grow to accept attribute ids alongside ints.
- [ ] **16. Inline `expect` / `assert` fences.** Turn runbooks into test suites. `expect exit 0`, `expect stdout contains "ok"`.
- [ ] **17. Pending-wait timeout reaper.** `on_timeout: abort` never fires until a human manually resumes. Background reaper (or auto-fire on next `wb pending`).
- [ ] **18. Source-hash execution cache.** Skip blocks whose source + env + inputs haven't changed. Massive for iterative agent re-runs.

## 🌐 Browser runtime

- [ ] **19. `wait_for_network_idle` verb.** Every SPA workbook is fragile without this.
- [ ] **20. Text-fallback selectors.** `click: { selector, text_fallback: "Send" }`. One change, ~half fewer brittle workbooks.
- [ ] **21. Auto-screenshot + console buffer on verb failure.** Current failures = single line of stderr; post-hoc debugging is impossible.
- [ ] **22. `WB_BROWSER_MODE=local` fallback.** Dev iteration without Browserbase cost/latency.
- [ ] **23. Structured error codes in sidecar events.** `SELECTOR_NOT_FOUND`, `NAV_TIMEOUT`, `AUTH_FAILED` — not freeform strings.

## 🧹 Code-health (do alongside, not instead)

- [ ] **24. Extract `run_single()`** (`src/main.rs:761-1455` — 700 lines, 13 params). Will become painful as 6/7/13/14/15 all touch execution dispatch.
- [ ] **25. Unified error type** (thiserror) — before all the new structured-error work multiplies the current `anyhow`/`String`/`unwrap` mix.
- [ ] **26. Type the sidecar↔checkpoint↔pending state** instead of opaque `serde_yaml::Value` — will otherwise become a scavenger hunt once browser recording metadata needs to survive pause/resume.

---

## Sequencing

- **Wave 1 — foundation (this session):** 1, 2, 3, 4, 5. All small, independent, no structural changes.
- **Wave 2 — agent-UX wins:** 6, 7, 8, 9 (then 10, 11, 12 as capacity allows).
- **Wave 3 — fence-attr foundation:** 13 first, then 6/7 refinements.
- **Wave 4 — power-ups:** 14 (params), 15 (retry/timeout), 16 (assertions).
- **Parallel track — browser:** 19-23 in any order; independent of core.
- **Health track:** 24-26 interleave with whichever wave touches the same files.

## Notes

- Silent/no-run flags were flagged as half-implemented by one audit; verified **fully wired** in `src/parser.rs` + `src/main.rs:1234, 1375`. No action needed.
- SIGKILL-on-shutdown race in sidecar was already fixed in commit `fee72a3`.
- `features-request.md` at repo root holds longer-form specs for fence-flags and browser recording — keep as canonical reference, this file is the checklist.
- v0.9.8 promoted four experimental flags to stable — `WB_EXPERIMENTAL_BLOCK_FLAGS`, `WB_EXPERIMENTAL_WAIT`, `WB_EXPERIMENTAL_SANDBOX`, `WB_EXPERIMENTAL_BROWSER` all removed. `{no-run}`/`{silent}`, `wait`/`resume`, sandbox, and browser blocks now work without opt-in env vars.
